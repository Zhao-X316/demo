import { useEffect, useRef, useState } from "react";
import type { Class } from "../../api/manage";
import type { WorkspaceTask } from "../../api/workspace";
import { workspaceTasks } from "../../api/workspace";
import Exam from "../Exam";
import Today from "../Today";
import GradingDesk from "../GradingDesk";
import Records from "../Records";
import WorkspaceHome from "./WorkspaceHome";
import { parseTaskLocator, taskKey } from "./taskModel";

export interface WorkRequest {
  id: number;
  screen: "exam" | "recitation" | "today" | "records";
}
interface Props {
  active: boolean;
  request?: WorkRequest;
  classes: Class[];
  classId: number;
  onClassChange: (id: number) => void;
  onOpenMaterials: () => void;
}
type Screen = "home" | "new" | "exam" | "recitation" | "records" | "today";
const LOCATION_KEY = "jiaofu.workspace.task.v1";

export default function Workspace({
  active,
  request,
  classes,
  classId,
  onClassChange,
  onOpenMaterials,
}: Props) {
  const generationRef = useRef(0);
  const [refreshTick, setRefreshTick] = useState(0);
  useEffect(() => {
    if (active) setRefreshTick((tick) => tick + 1);
  }, [active]);
  const dirty = useRef(false);
  const [hasDraft, setHasDraft] = useState(false);
  function setDirty(value: boolean) {
    dirty.current = value;
    setHasDraft(value);
  }
  function mayReplace() {
    return (
      !dirty.current ||
      window.confirm(
        "当前还有未保存的输入。离开后将舍弃这些输入，已保存的材料和评分会保留。继续离开？",
      )
    );
  }
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (dirty.current) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, []);
  const [activeScreen, setActiveScreen] = useState<Screen>("home");
  const [screen, setScreen] = useState<Screen>("home");
  const [task, setTask] = useState<WorkspaceTask | null>(null);
  const [generation, setGeneration] = useState(0);
  const [error, setError] = useState("");
  const [restoring, setRestoring] = useState(true);
  function remember(next: WorkspaceTask | null) {
    try {
      if (next) localStorage.setItem(LOCATION_KEY, taskKey(next));
      else localStorage.removeItem(LOCATION_KEY);
    } catch {
      /* Storage is optional; domain records remain available. */
    }
  }
  function open(next: WorkspaceTask) {
    if (task && taskKey(next) === taskKey(task)) {
      setScreen(activeScreen);
      return;
    }
    if (!mayReplace()) return;
    setDirty(false);
    setTask(next);
    remember(next);
    setGeneration(++generationRef.current);
    setError("");
    const destination =
      next.kind !== "recitation"
        ? "exam"
        : next.hasEvidence &&
            !["anomaly", "recognition_failed"].includes(next.status)
          ? "records"
          : "recitation";
    setScreen(destination);
    setActiveScreen(destination);
  }
  useEffect(() => {
    let active = true;
    const openingGeneration = generationRef.current;
    async function restore() {
      try {
        const locator = parseTaskLocator(localStorage.getItem(LOCATION_KEY));
        if (locator) {
          const rows = await workspaceTasks();
          const saved = rows.find((row) => taskKey(row) === taskKey(locator));
          if (active && openingGeneration === generationRef.current) {
            if (saved) open(saved);
            else {
              remember(null);
              setError("上次的任务已不可用，请从工作台重新选择。");
            }
          }
        }
      } catch (reason) {
        if (active)
          setError(
            `未能恢复上次位置：${String(reason)}。已保存任务可重新读取。`,
          );
      } finally {
        if (active) setRestoring(false);
      }
    }
    void restore();
    return () => {
      active = false;
    };
  }, []);
  function home() {
    setScreen("home");
  }
  function start(destination: Screen) {
    if (!mayReplace()) return;
    setDirty(false);
    generationRef.current += 1;
    setGeneration(generationRef.current);
    setTask(null);
    remember(null);
    setScreen(destination);
    setActiveScreen(destination);
  }
  useEffect(() => {
    if (!request) return;
    // A page switch does not replace the current upload/review component.
    if (request.screen === activeScreen) {
      setScreen(activeScreen);
      return;
    }
    start(request.screen);
  }, [request?.id]);
  if (restoring) return <div className="page loading">读取工作台…</div>;
  return (
    <div
      onChangeCapture={(event) => {
        if (
          screen !== "home" &&
          (event.target instanceof HTMLInputElement ||
            event.target instanceof HTMLTextAreaElement)
        )
          setDirty(true);
      }}
    >
      {hasDraft && screen !== "home" && (
        <p className="workspace-back">
          有尚未保存的输入；返回工作台或切换主导航会保留，刷新前请先保存。
        </p>
      )}
      {screen !== "home" && (
        <div className="workspace-back">
          <button className="link" onClick={home}>
            ← 返回工作台
          </button>
          {task && (
            <span>
              {task.className ?? "待匹配班级"} · {task.title}
            </span>
          )}
        </div>
      )}
      {screen === "home" && activeScreen !== "home" && (
        <div className="workspace-back">
          <button onClick={() => setScreen(activeScreen)}>
            继续刚才的操作
          </button>
          <span>本次未保存的输入仍保留在页面中</span>
        </div>
      )}
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      {screen === "home" && (
        <WorkspaceHome
          refreshTick={refreshTick}
          classes={classes}
          classId={classId}
          onClassChange={onClassChange}
          onOpen={open}
          onNew={() => setScreen("new")}
          onSchedule={() => start("today")}
          onHistory={() => start("records")}
        />
      )}
      {screen === "new" && (
        <div className="page workspace-new">
          <h1>新建批改</h1>
          <p className="sub">选择这次要处理的材料</p>
          <div className="workspace-type-choices">
            <button aria-label="作业" onClick={() => start("exam")}>
              作业<span>学生试卷图片、PDF 或默写</span>
            </button>
            <button aria-label="背诵" onClick={() => start("recitation")}>
              背诵<span>导入学生录音，核对评分点</span>
            </button>
          </div>
          <button className="link" onClick={onOpenMaterials}>
            先准备题目或背诵内容 →
          </button>
        </div>
      )}
      {activeScreen === "exam" && (
        <div hidden={screen !== "exam"}>
          <Exam
            refreshTick={refreshTick}
            onDirtyChange={setDirty}
            key={generation}
            task={task}
            initialClassId={classId}
            onOpenMaterials={onOpenMaterials}
            onBatchSaved={(batchId) => {
              setDirty(false);
              const token = generation;
              void workspaceTasks()
                .then((rows) => {
                  if (token !== generationRef.current) return;
                  const saved = rows.find(
                    (row) =>
                      row.kind === "exam_batch" && row.sourceId === batchId,
                  );
                  if (saved) {
                    setTask(saved);
                    remember(saved);
                  }
                })
                .catch((reason) => {
                  if (token === generationRef.current)
                    setError(
                      `批次已保存，但任务列表刷新失败：${String(reason)}`,
                    );
                });
            }}
          />
        </div>
      )}
      {activeScreen === "recitation" && (
        <div hidden={screen !== "recitation"}>
          <GradingDesk
            onDirtyChange={setDirty}
            key={generation}
            initialSubmissionId={task?.sourceId}
          />
        </div>
      )}
      {activeScreen === "records" && (
        <div hidden={screen !== "records"}>
          <Records
            key={generation}
            initialSubmissionId={
              task?.kind === "recitation" ? task.sourceId : undefined
            }
          />
        </div>
      )}
      {activeScreen === "today" && (
        <div hidden={screen !== "today"}>
          <Today />
        </div>
      )}
    </div>
  );
}
