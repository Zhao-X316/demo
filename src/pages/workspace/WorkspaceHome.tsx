import { useEffect, useState } from "react";
import type { Class } from "../../api/manage";
import { workspaceTasks, type WorkspaceTask } from "../../api/workspace";
import {
  taskAction,
  taskFilter,
  taskKey,
  taskLabel,
  type TaskFilter,
} from "./taskModel";

interface Props {
  refreshTick?: number;
  classes: Class[];
  classId: number;
  onClassChange: (id: number) => void;
  onOpen: (task: WorkspaceTask) => void;
  onNew: () => void;
  onSchedule: () => void;
  onHistory: () => void;
}

export default function WorkspaceHome({
  refreshTick,
  classes,
  classId,
  onClassChange,
  onOpen,
  onNew,
  onSchedule,
  onHistory,
}: Props) {
  const [tasks, setTasks] = useState<WorkspaceTask[]>([]);
  const [filter, setFilter] = useState<TaskFilter>("todo");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [visibleCount, setVisibleCount] = useState(30);
  async function refresh() {
    setLoading(true);
    setError("");
    try {
      const rows = await workspaceTasks();
      if (!Array.isArray(rows))
        throw new Error("无法读取任务列表，请检查桌面服务。");
      setTasks(rows);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    void refresh();
  }, [refreshTick]);
  const filtered = tasks
    .filter(
      (task) =>
        (!classId || task.classId === classId || task.classId === null) &&
        taskFilter(task) === filter,
    )
    .sort(
      (a, b) =>
        Number(
          ["recognition_failed", "anomaly", "incomplete"].includes(b.status),
        ) -
        Number(
          ["recognition_failed", "anomaly", "incomplete"].includes(a.status),
        ),
    );
  return (
    <div className="page workspace-home">
      <div className="page-head">
        <div>
          <h1>工作台</h1>
          <label className="workspace-class">
            当前班级{" "}
            <select
              aria-label="工作台班级"
              value={classId}
              onChange={(event) => {
                onClassChange(Number(event.target.value));
                setVisibleCount(30);
              }}
            >
              <option value={0}>全部班级</option>
              {classes.map((row) => (
                <option key={row.id} value={row.id}>
                  {row.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <button className="primary" onClick={onNew}>
          新建批改
        </button>
      </div>
      <div className="workspace-toolbar">
        <div className="tabs" aria-label="任务状态">
          {(
            [
              ["todo", "待我处理"],
              ["processing", "处理中"],
              ["done", "已完成"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              className={filter === value ? "tab active" : "tab"}
              aria-pressed={filter === value}
              onClick={() => {
                setFilter(value);
                setVisibleCount(30);
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <button
          className="link"
          disabled={loading}
          onClick={() => void refresh()}
        >
          刷新
        </button>
      </div>
      {error && (
        <div className="error" role="alert">
          {error} <button onClick={() => void refresh()}>重新读取</button>
        </div>
      )}
      {loading ? (
        <div className="loading">读取已保存的任务…</div>
      ) : !error && filtered.length === 0 ? (
        <div className="workspace-empty">
          <h2>
            {filter === "todo"
              ? "当前没有待处理的批改"
              : filter === "processing"
                ? "没有正在处理的任务"
                : "还没有已完成的批改"}
          </h2>
          <p>新任务可以从右上角开始，已有背诵安排可在下方查看。</p>
        </div>
      ) : (
        <div className="workspace-task-list">
          {filtered.slice(0, visibleCount).map((task) => (
            <article className="workspace-task" key={taskKey(task)}>
              <div>
                <h2>{task.title}</h2>
                <p>
                  {task.className ?? "待匹配班级"}
                  {task.studentName ? ` · ${task.studentName}` : ""} ·{" "}
                  {task.kind === "recitation" ? "背诵" : "作业"}
                </p>
                <span className={`workspace-status ${taskFilter(task)}`}>
                  {taskLabel(task)}
                </span>
              </div>
              <button className="link" onClick={() => onOpen(task)}>
                {taskAction(task)} →
              </button>
            </article>
          ))}
        </div>
      )}
      {filtered.length > visibleCount && (
        <button
          className="workspace-more"
          onClick={() => setVisibleCount((count) => count + 30)}
        >
          再显示 30 项（共 {filtered.length} 项）
        </button>
      )}
      <div className="workspace-utilities">
        <button className="link" onClick={onSchedule}>
          背诵安排
        </button>
        <button className="link" onClick={onHistory}>
          录音导入记录
        </button>
      </div>
    </div>
  );
}
