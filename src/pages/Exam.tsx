import { useEffect, useRef, useState } from "react";
import {
  AnswerDetail,
  DictationWorkbench,
  FixedIntakeOption,
  KnowledgePoint,
  ObjectiveWorkbench,
  SubjectiveWorkbench,
  Question,
  examAnswerSheetSubjectiveWorkbench,
  examAnswersList,
  examDictationWorkbench,
  examFixedIntakeOptions,
  examObjectiveWorkbench,
  kpList,
  questionsList,
} from "../api/exam";
import { Student, studentsList } from "../api/manage";
import { DictationReviewTab } from "./exam/DictationReviewTab";
import { FixedIntakeTab } from "./exam/FixedIntakeTab";
import { GradeTab } from "./exam/GradeTab";
import { KnowledgeTab } from "./exam/KnowledgeTab";
import { ObjectiveReviewTab } from "./exam/ObjectiveReviewTab";
import { QuestionTab } from "./exam/QuestionTab";
import { SubjectiveReviewTab } from "./exam/SubjectiveReviewTab";

import {
  resumeIntake,
  workspaceExamReview,
  type IntakeResume,
  type WorkspaceTask,
} from "../api/workspace";
import { scopeWorkbench } from "./workspace/taskModel";
import TaskReview from "./workspace/TaskReview";
import TaskResults from "./workspace/TaskResults";

type Tab =
  | "processing"
  | "review"
  | "results"
  | "intake"
  | "objective"
  | "subjective"
  | "dictation"
  | "grade"
  | "questions"
  | "knowledge";

interface Props {
  refreshTick?: number;
  onDirtyChange?: (dirty: boolean) => void;
  task?: WorkspaceTask | null;
  initialClassId?: number;
  initialTool?: "questions" | "knowledge";
  onOpenMaterials?: () => void;
  onBatchSaved?: (batchId: number) => void;
}
export default function Exam({
  refreshTick,
  task = null,
  initialClassId,
  initialTool,
  onOpenMaterials,
  onBatchSaved,
  onDirtyChange,
}: Props) {
  const initialTask = useRef(task).current;
  const batchIdRef = useRef(task?.kind === "exam_batch" ? task.sourceId : null);
  const [currentTask, setCurrentTask] = useState(task);
  const [resume, setResume] = useState<IntakeResume>();
  const [tab, setTab] = useState<Tab>(
    initialTool ??
      (task?.status === "published" || task?.status === "ready_to_publish"
        ? "results"
        : task?.hasEvidence
          ? "review"
          : "intake"),
  );
  const readGeneration = useRef(0);
  const [students, setStudents] = useState<Student[]>([]);
  const [questions, setQuestions] = useState<Question[]>([]);
  const [knowledge, setKnowledge] = useState<KnowledgePoint[]>([]);
  const [answers, setAnswers] = useState<AnswerDetail[]>([]);
  const [objectiveWorkbench, setObjectiveWorkbench] =
    useState<ObjectiveWorkbench>({ rows: [], attempts: [] });
  const [subjectiveWorkbench, setSubjectiveWorkbench] =
    useState<SubjectiveWorkbench>({ rows: [], attempts: [] });
  const [dictationWorkbench, setDictationWorkbench] =
    useState<DictationWorkbench>({ rows: [], attempts: [] });
  const [intakeOptions, setIntakeOptions] = useState<FixedIntakeOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [resumeError, setResumeError] = useState("");
  const [readFailed, setReadFailed] = useState(false);
  const [toast, setToast] = useState("");

  const load = async () => {
    setError("");
    const generation = ++readGeneration.current;
    try {
      const locator = batchIdRef.current
        ? { kind: "exam_batch" as const, sourceId: batchIdRef.current }
        : initialTask;
      const snapshot = locator
        ? await workspaceExamReview(locator.kind, locator.sourceId)
        : null;
      if (
        locator &&
        (!snapshot?.task ||
          snapshot.task.sourceId !== locator.sourceId ||
          snapshot.task.kind !== locator.kind)
      )
        throw new Error("任务读取结果不匹配，请重新打开本次任务");
      const scope = snapshot?.task ?? null;
      const [
        studentRows,
        questionRows,
        kpRows,
        answerRows,
        workbench,
        subjectiveRows,
        dictationRows,
        fixedOptions,
      ] = await Promise.all([
        studentsList(),
        questionsList(),
        kpList(),
        examAnswersList(),
        snapshot?.objective ?? examObjectiveWorkbench(),
        snapshot?.subjective ?? examAnswerSheetSubjectiveWorkbench(),
        snapshot?.dictation ?? examDictationWorkbench(),
        examFixedIntakeOptions(),
      ]);
      if (readGeneration.current !== generation) return;
      setReadFailed(false);
      setCurrentTask(scope);
      setStudents(studentRows.filter((student) => student.enabled));
      setQuestions(questionRows.filter((question) => question.enabled));
      setKnowledge(kpRows);
      setAnswers(answerRows);
      setObjectiveWorkbench(workbench);
      setSubjectiveWorkbench(subjectiveRows);
      setDictationWorkbench(dictationRows);
      setIntakeOptions(fixedOptions);
    } catch (err) {
      if (readGeneration.current === generation) {
        setReadFailed(true);
        setError(String(err));
      }
    } finally {
      if (readGeneration.current === generation) setLoading(false);
    }
  };

  useEffect(() => {
    let active = true;
    async function start() {
      if (initialTask?.kind === "exam_batch") {
        try {
          const saved = await resumeIntake(initialTask.sourceId);
          if (!active) return;
          setResume(saved);
        } catch (reason) {
          if (!active) return;
          setResumeError(
            `材料入口暂不能恢复：${String(reason)}。已有识别与评分仍可核对；需要重新导入时，请从工作台另建批改，原记录会保留。`,
          );
        }
      }
      if (active) await load();
    }
    void start();
    return () => {
      active = false;
      readGeneration.current += 1;
    };
  }, []);

  const lastRefresh = useRef(refreshTick);
  useEffect(() => {
    if (lastRefresh.current !== refreshTick) {
      lastRefresh.current = refreshTick;
      void load();
    }
  }, [refreshTick]);

  const done = (message: string) => {
    onDirtyChange?.(false);
    setToast(message);
    setError("");
    load();
  };

  const scopeReady =
    !batchIdRef.current ||
    (currentTask?.kind === "exam_batch" &&
      currentTask.sourceId === batchIdRef.current);
  const scopedObjective = {
    rows: scopeReady
      ? scopeWorkbench(objectiveWorkbench.rows, currentTask)
      : [],
    attempts: scopeReady
      ? scopeWorkbench(objectiveWorkbench.attempts, currentTask)
      : [],
  };
  const scopedSubjective = {
    rows: scopeReady
      ? scopeWorkbench(subjectiveWorkbench.rows, currentTask)
      : [],
    attempts: scopeReady
      ? scopeWorkbench(subjectiveWorkbench.attempts, currentTask)
      : [],
  };
  const scopedDictation = {
    rows: scopeReady
      ? scopeWorkbench(dictationWorkbench.rows, currentTask)
      : [],
    attempts: scopeReady
      ? scopeWorkbench(dictationWorkbench.attempts, currentTask)
      : [],
  };
  const savedBatch = (batchId: number) => {
    const changed = batchIdRef.current !== batchId;
    batchIdRef.current = batchId;
    if (changed) void load();
    if (!initialTask) setTab("processing");
    onBatchSaved?.(batchId);
  };
  return (
    <div className="page exam-page">
      <div className="page-head">
        <div>
          <h1>题目批改</h1>
          <div className="sub">上传材料，处理疑点，确认后再发布。</div>
        </div>
        {onOpenMaterials && (
          <button onClick={onOpenMaterials}>准备题目与答案</button>
        )}
      </div>
      {currentTask && scopeReady && (
        <div className="task-scope">
          {currentTask.className} · {currentTask.title} ·
          本次范围固定，切换班级不会改动本批。
        </div>
      )}
      <nav className="task-step-nav" aria-label="批改步骤">
        <button
          className={tab === "intake" ? "active" : ""}
          onClick={() => setTab("intake")}
        >
          1 资料
        </button>
        <button
          className={tab === "processing" ? "active" : ""}
          onClick={() => setTab("processing")}
        >
          2 系统处理
        </button>
        <button
          className={tab === "review" ? "active" : ""}
          onClick={() => {
            void load();
            setTab("review");
          }}
        >
          3 老师核对
        </button>
        <button
          className={tab === "results" ? "active" : ""}
          onClick={() => {
            void load();
            setTab("results");
          }}
        >
          4 结果
        </button>
      </nav>
      <details className="task-tools">
        <summary>更多批改工具</summary>
        <div className="tabs">
          <button
            className={tab === "objective" ? "tab active" : "tab"}
            onClick={() => setTab("objective")}
          >
            标准卷终审
          </button>
          <button
            className={tab === "subjective" ? "tab active" : "tab"}
            onClick={() => setTab("subjective")}
          >
            答题卡主观题
          </button>
          <button
            className={tab === "dictation" ? "tab active" : "tab"}
            onClick={() => setTab("dictation")}
          >
            默写复核
          </button>
          {!currentTask && (
            <>
              <button
                className={tab === "grade" ? "tab active" : "tab"}
                onClick={() => setTab("grade")}
              >
                老师补录
              </button>
              <button
                className={tab === "questions" ? "tab active" : "tab"}
                onClick={() => setTab("questions")}
              >
                题库
              </button>
              <button
                className={tab === "knowledge" ? "tab active" : "tab"}
                onClick={() => setTab("knowledge")}
              >
                知识点
              </button>
            </>
          )}
        </div>
      </details>
      {error && <div className="error">{error}</div>}
      {toast && <div className="ok-banner">{toast}</div>}
      {readFailed ? (
        <div className="workspace-empty">
          本次数据未完整读取，已暂停批改入口。返回工作台后可重新打开；已保存的材料仍保留。
        </div>
      ) : loading ? (
        <div className="loading">加载题目批改数据…</div>
      ) : (
        <>
          <div hidden={tab !== "intake" && tab !== "processing"}>
            {resumeError ? (
              <div className="error" role="alert">
                {resumeError}
              </div>
            ) : (
              <FixedIntakeTab
                initialClassId={initialClassId}
                resume={resume}
                onBatchSaved={savedBatch}
                onDirtyChange={onDirtyChange}
                options={intakeOptions}
                onOpenReview={() => {
                  void load();
                  setTab("review");
                }}
                onDone={done}
                onError={(message) => setError(message)}
              />
            )}
          </div>
          {currentTask && scopeReady && (
            <div hidden={tab !== "review"}>
              <TaskReview
                objective={scopedObjective}
                subjective={scopedSubjective}
                dictation={scopedDictation}
                onDone={done}
                onError={setError}
              />
            </div>
          )}
          {tab === "review" && !currentTask && (
            <div className="workspace-empty">
              请先上传材料，或从工作台打开已保存的任务。
            </div>
          )}
          {tab === "results" &&
            (currentTask && scopeReady ? (
              <TaskResults
                objective={scopedObjective}
                subjective={scopedSubjective}
                dictation={scopedDictation}
                onDone={done}
                onError={setError}
              />
            ) : (
              <div className="workspace-empty">
                完成本次材料处理后，可以在这里查看结果。
              </div>
            ))}
          {tab === "objective" && (
            <ObjectiveReviewTab
              workbench={scopedObjective}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "subjective" && (
            <SubjectiveReviewTab
              workbench={scopedSubjective}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "dictation" && (
            <DictationReviewTab
              workbench={scopedDictation}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "grade" && (
            <GradeTab
              students={students}
              questions={questions}
              answers={answers}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "questions" && (
            <QuestionTab
              questions={questions}
              knowledge={knowledge}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
          {tab === "knowledge" && (
            <KnowledgeTab
              knowledge={knowledge}
              onDone={done}
              onError={(message) => setError(message)}
            />
          )}
        </>
      )}
    </div>
  );
}
