import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  AnswerDetail,
  KnowledgePoint,
  ObjectiveWorkbench,
  ObjectiveWorkbenchRow,
  Question,
  QuestionInput,
  examAnswerHumanDecide,
  examAnswerSuggest,
  examAnswersList,
  examObjectiveAccept,
  examObjectiveCorrect,
  examObjectivePublishAttempt,
  examObjectiveStrictBatchAccept,
  examObjectiveWorkbench,
  kpCreate,
  kpList,
  questionCreate,
  questionsList,
} from "../api/exam";
import { Student, studentsList } from "../api/manage";

type Tab = "objective" | "grade" | "questions" | "knowledge";

const TYPE_LABEL: Record<string, string> = {
  single: "单选",
  multi: "多选",
  judge: "判断",
  fill: "填空",
  subjective: "主观",
};

const OBJECTIVE_TYPE_LABEL: Record<string, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
};

const OBSERVATION_LABEL: Record<string, string> = {
  recognized: "识别清晰",
  blank: "疑似空白",
  altered: "存在涂改",
  low_confidence: "低置信度",
  failed: "识别失败",
};

const EXCLUSION_LABEL: Record<string, string> = {
  ALREADY_CONFIRMED: "已终审",
  OBSERVATION_NOT_RECOGNIZED: "识别状态不合格",
  LOW_CONFIDENCE: "置信度不足",
  NOT_BATCH_ELIGIBLE: "不满足批量规则",
  UNSCORED: "机器无法确定计分",
};

function displayTime(value: string) {
  return value.replace("T", " ").slice(0, 16);
}

export default function Exam() {
  const [tab, setTab] = useState<Tab>("grade");
  const [students, setStudents] = useState<Student[]>([]);
  const [questions, setQuestions] = useState<Question[]>([]);
  const [knowledge, setKnowledge] = useState<KnowledgePoint[]>([]);
  const [answers, setAnswers] = useState<AnswerDetail[]>([]);
  const [objectiveWorkbench, setObjectiveWorkbench] = useState<ObjectiveWorkbench>({ rows: [], attempts: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");

  const load = async () => {
    setError("");
    try {
      const [studentRows, questionRows, kpRows, answerRows, workbench] = await Promise.all([
        studentsList(),
        questionsList(),
        kpList(),
        examAnswersList(),
        examObjectiveWorkbench(),
      ]);
      setStudents(studentRows.filter((student) => student.enabled));
      setQuestions(questionRows.filter((question) => question.enabled));
      setKnowledge(kpRows);
      setAnswers(answerRows);
      setObjectiveWorkbench(workbench);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const done = (message: string) => {
    setToast(message);
    setError("");
    load();
  };

  return (
    <div className="page exam-page">
      <div className="page-head">
        <div>
          <h1>题目批改</h1>
          <div className="sub">机器先建议，老师确认后才计分并进入错题与掌握度。</div>
        </div>
        <span className="tag makeup">M2 · 客观题终审</span>
      </div>
      <div className="exam-notice">
        <b>当前边界</b>
        <span>固定模板客观题已接入按题终审、严格批量确认和显式发布；真实 OMR 识别仍未接入，手工录入保留为兜底。</span>
      </div>
      <div className="tabs">
        <button className={tab === "objective" ? "tab active" : "tab"} onClick={() => setTab("objective")}>标准卷终审</button>
        <button className={tab === "grade" ? "tab active" : "tab"} onClick={() => setTab("grade")}>快速批改</button>
        <button className={tab === "questions" ? "tab active" : "tab"} onClick={() => setTab("questions")}>题库</button>
        <button className={tab === "knowledge" ? "tab active" : "tab"} onClick={() => setTab("knowledge")}>知识点</button>
      </div>
      {error && <div className="error">{error}</div>}
      {toast && <div className="ok-banner">{toast}</div>}
      {loading ? <div className="loading">加载题目批改数据…</div> : (
        <>
          {tab === "objective" && (
            <ObjectiveReviewTab
              workbench={objectiveWorkbench}
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

function displayObservedAnswer(row: ObjectiveWorkbenchRow) {
  if (!row.observed_answer_json) {
    return row.observation_state === "blank" ? "空白" : "—";
  }
  try {
    const value = JSON.parse(row.observed_answer_json) as {
      selected?: boolean;
      selected_labels?: string[];
      selected_values?: boolean[];
    };
    if (typeof value.selected === "boolean") {
      return value.selected ? "正确（√）" : "错误（×）";
    }
    if (Array.isArray(value.selected_labels)) {
      return value.selected_labels.join("、") || "—";
    }
    if (Array.isArray(value.selected_values)) {
      return value.selected_values.map((selected) => selected ? "√" : "×").join("、") + "（冲突）";
    }
  } catch {
    return row.observed_answer_json;
  }
  return row.observed_answer_json;
}

function objectiveOutcomeLabel(row: ObjectiveWorkbenchRow) {
  if (row.suggestion_outcome === "correct") return "建议正确";
  if (row.suggestion_outcome === "incorrect") return "建议错误";
  return "无法计分";
}

function ObjectiveReviewTab({
  workbench,
  onDone,
  onError,
}: {
  workbench: ObjectiveWorkbench;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const assessmentVersions = useMemo(() => {
    const unique = new Map<number, string>();
    workbench.rows.forEach((row) => unique.set(row.assessment_version_id, row.assessment_title));
    return Array.from(unique, ([id, title]) => ({ id, title }));
  }, [workbench.rows]);
  const [assessmentVersionId, setAssessmentVersionId] = useState(assessmentVersions[0]?.id ?? 0);
  const [assessmentItemId, setAssessmentItemId] = useState(0);
  const [busy, setBusy] = useState(false);
  const [manualScores, setManualScores] = useState<Record<number, string>>({});
  const [manualNotes, setManualNotes] = useState<Record<number, string>>({});

  useEffect(() => {
    if (!assessmentVersions.some((version) => version.id === assessmentVersionId)) {
      setAssessmentVersionId(assessmentVersions[0]?.id ?? 0);
    }
  }, [assessmentVersionId, assessmentVersions]);

  const itemOptions = useMemo(() => {
    const unique = new Map<number, ObjectiveWorkbenchRow>();
    workbench.rows
      .filter((row) => row.assessment_version_id === assessmentVersionId)
      .forEach((row) => unique.set(row.assessment_item_id, row));
    return Array.from(unique.values()).sort((left, right) => left.order_index - right.order_index);
  }, [assessmentVersionId, workbench.rows]);

  useEffect(() => {
    if (!itemOptions.some((row) => row.assessment_item_id === assessmentItemId)) {
      setAssessmentItemId(itemOptions[0]?.assessment_item_id ?? 0);
    }
  }, [assessmentItemId, itemOptions]);

  const rows = workbench.rows
    .filter((row) => row.assessment_version_id === assessmentVersionId && row.assessment_item_id === assessmentItemId)
    .sort((left, right) => left.student_no.localeCompare(right.student_no, "zh-CN", { numeric: true }));
  const attempts = workbench.attempts.filter((attempt) => attempt.assessment_version_id === assessmentVersionId);
  const confirmedCount = rows.filter((row) => row.current_suggestion_confirmed).length;
  const eligibleCount = rows.filter((row) => row.batch_eligible && !row.current_suggestion_confirmed).length;
  const exceptionCount = rows.length - confirmedCount - eligibleCount;

  const acceptOne = async (row: ObjectiveWorkbenchRow) => {
    setBusy(true);
    try {
      await examObjectiveAccept(row.suggestion_id);
      onDone(`已确认 ${row.student_name} 的第 ${row.order_index} 题，成绩仍需整卷显式发布`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const correctOne = async (row: ObjectiveWorkbenchRow) => {
    const rawScore = manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "");
    const score = Number(rawScore);
    const note = (manualNotes[row.suggestion_id] ?? "").trim();
    if (!Number.isFinite(score) || score < 0 || score > row.max_score) {
      onError(`人工得分必须位于 0~${row.max_score} 分`);
      return;
    }
    if (!note) {
      onError("人工记分必须填写查看原图后的证据依据");
      return;
    }
    setBusy(true);
    try {
      await examObjectiveCorrect(row.suggestion_id, score, note);
      onDone(`已人工确认 ${row.student_name} 的第 ${row.order_index} 题为 ${score} 分`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const acceptStrictBatch = async () => {
    if (eligibleCount === 0) return;
    setBusy(true);
    try {
      const batch = await examObjectiveStrictBatchAccept(
        rows.map((row) => row.suggestion_id),
        `objective-review-${crypto.randomUUID()}`,
      );
      onDone(`严格批量终审完成：确认 ${batch.confirmed_count} 条，排除 ${batch.excluded_count} 条异常`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const publishAttempt = async (attemptId: number, studentName: string) => {
    if (!window.confirm(`确认发布 ${studentName} 的本次整卷成绩？发布只采用当前老师终审 revision。`)) return;
    setBusy(true);
    try {
      const publication = await examObjectivePublishAttempt(attemptId);
      onDone(`${studentName} 的成绩已发布：${publication.total_score} 分（发布 revision ${publication.revision}）`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  if (workbench.rows.length === 0) {
    return (
      <div className="empty-state objective-empty">
        <b>还没有可终审的标准卷客观题。</b>
        <span>当前只读取已完成页面匹配、配准、答案区域确认和固定夹具观察的数据；真实 OMR 识别尚未接入。</span>
        <span>需要临时录入时可继续使用“快速批改”兜底。</span>
      </div>
    );
  }

  return (
    <>
      <section className="objective-toolbar exam-card">
        <label className="field">
          <span className="fl">作业版本</span>
          <select value={assessmentVersionId} onChange={(event) => setAssessmentVersionId(Number(event.target.value))}>
            {assessmentVersions.map((version) => <option value={version.id} key={version.id}>{version.title} · v{version.id}</option>)}
          </select>
        </label>
        <label className="field">
          <span className="fl">按题终审</span>
          <select value={assessmentItemId} onChange={(event) => setAssessmentItemId(Number(event.target.value))}>
            {itemOptions.map((row) => (
              <option value={row.assessment_item_id} key={row.assessment_item_id}>
                第 {row.order_index} 题 · {OBJECTIVE_TYPE_LABEL[row.question_type]} · {row.question_stem.slice(0, 34)}
              </option>
            ))}
          </select>
        </label>
        <div className="objective-stats">
          <span><b>{rows.length}</b> 份作答</span>
          <span className="ok-text"><b>{confirmedCount}</b> 已确认</span>
          <span><b>{eligibleCount}</b> 可严格批量</span>
          <span className={exceptionCount ? "bad-text" : ""}><b>{exceptionCount}</b> 需单独处理</span>
        </div>
        <button className="primary" disabled={busy || eligibleCount === 0} onClick={acceptStrictBatch}>
          {busy ? "处理中…" : `确认 ${eligibleCount} 条高置信度结果`}
        </button>
        <div className="meta objective-batch-note">阈值固定为 0.95；空白、涂改、低置信度、识别失败和已终审记录都会明确排除。</div>
      </section>

      <div className="sech">本题证据 <span className="n">按学号排序</span></div>
      <div className="objective-review-list">
        {rows.map((row) => {
          const canAccept = row.suggested_score != null && !row.current_suggestion_confirmed;
          return (
            <article className={row.current_suggestion_confirmed ? "objective-review-row confirmed" : "objective-review-row"} key={row.suggestion_id}>
              <div className="objective-student">
                <b>{row.student_name}</b>
                <span>{row.student_no}</span>
                <span className={row.current_suggestion_confirmed ? "tag pass" : row.batch_eligible ? "tag wait" : "tag fail"}>
                  {row.current_suggestion_confirmed ? "已终审" : row.batch_eligible ? "可严格批量" : "需单独处理"}
                </span>
              </div>
              <div className="objective-evidence">
                {row.crop_path ? (
                  <img className="objective-crop" src={convertFileSrc(row.crop_path)} alt={`${row.student_name} 第 ${row.order_index} 题答案区域`} />
                ) : (
                  <div className="objective-crop missing">无答案裁剪</div>
                )}
                <div className="objective-facts">
                  <span>观察结果 <b>{OBSERVATION_LABEL[row.observation_state] ?? row.observation_state}</b></span>
                  <span>识别答案 <b>{displayObservedAnswer(row)}</b></span>
                  <span>置信度 <b>{row.confidence == null ? "—" : `${(row.confidence * 100).toFixed(1)}%`}</b></span>
                  <span>机器建议 <b className={row.suggestion_outcome === "correct" ? "ok-text" : row.suggestion_outcome === "incorrect" ? "bad-text" : ""}>{objectiveOutcomeLabel(row)}</b></span>
                  <span>建议得分 <b>{row.suggested_score == null ? "—" : `${row.suggested_score} / ${row.max_score}`}</b></span>
                  {row.exclusion_reason && <span>排除原因 <b>{EXCLUSION_LABEL[row.exclusion_reason] ?? row.exclusion_reason}</b></span>}
                  {row.current_suggestion_confirmed && <span>老师终审 <b>{row.teacher_score} 分 · {row.review_mode === "strict_batch" ? "严格批量" : "逐条确认"}</b></span>}
                </div>
              </div>
              <div className="objective-actions">
                <button className={canAccept ? "primary" : ""} disabled={busy || !canAccept} onClick={() => acceptOne(row)}>
                  {row.current_suggestion_confirmed ? "已确认" : row.suggested_score == null ? "机器无法计分" : "接受本条建议"}
                </button>
                {!row.current_suggestion_confirmed && (
                  <details>
                    <summary>人工记分</summary>
                    <label className="field">
                      <span className="fl">得分（满分 {row.max_score}）</span>
                      <input
                        type="number"
                        min="0"
                        max={row.max_score}
                        step="0.5"
                        value={manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "")}
                        onChange={(event) => setManualScores((current) => ({ ...current, [row.suggestion_id]: event.target.value }))}
                      />
                    </label>
                    <label className="field">
                      <span className="fl">证据依据</span>
                      <input
                        type="text"
                        placeholder="如：查看原图后确认涂改答案"
                        value={manualNotes[row.suggestion_id] ?? ""}
                        onChange={(event) => setManualNotes((current) => ({ ...current, [row.suggestion_id]: event.target.value }))}
                      />
                    </label>
                    <button disabled={busy} onClick={() => correctOne(row)}>保存人工 revision</button>
                  </details>
                )}
              </div>
            </article>
          );
        })}
      </div>

      <div className="sech">整卷发布 <span className="n">终审完成不等于已发布</span></div>
      <div className="objective-publish-grid">
        {attempts.map((attempt) => (
          <article className="exam-card objective-attempt" key={attempt.attempt_id}>
            <div className="exam-card-head">
              <div><b>{attempt.student_name}</b><div className="meta">{attempt.student_no}</div></div>
              <span className={attempt.attempt_state === "published" ? "tag pass" : attempt.can_publish ? "tag wait" : "tag"}>
                {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "待发布" : "终审中"}
              </span>
            </div>
            <div className="objective-attempt-score">
              <b>{attempt.teacher_total_score}</b><span>/ {attempt.max_total_score} 分</span>
            </div>
            <div className="meta">已终审 {attempt.confirmed_count} / {attempt.item_count} 题</div>
            {attempt.published_total_score != null && <div className="meta">当前已发布总分：{attempt.published_total_score}</div>}
            <button className="primary" disabled={busy || !attempt.can_publish} onClick={() => publishAttempt(attempt.attempt_id, attempt.student_name)}>
              {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "确认发布整卷" : "完成全部终审后发布"}
            </button>
          </article>
        ))}
      </div>
    </>
  );
}

function GradeTab({
  students,
  questions,
  answers,
  onDone,
  onError,
}: {
  students: Student[];
  questions: Question[];
  answers: AnswerDetail[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const objective = questions.filter((question) => question.qtype !== "subjective");
  const [studentId, setStudentId] = useState(students[0]?.id ?? 0);
  const [questionId, setQuestionId] = useState(objective[0]?.id ?? 0);
  const [picked, setPicked] = useState("");
  const [busy, setBusy] = useState(false);
  const [notes, setNotes] = useState<Record<number, string>>({});

  const selectedQuestion = objective.find((question) => question.id === questionId);
  const pending = answers.filter((answer) => answer.status === "pending_review").length;

  const suggest = async () => {
    if (!studentId || !questionId || !picked.trim()) {
      onError("请选择学生、题目并填写学生答案");
      return;
    }
    setBusy(true);
    try {
      await examAnswerSuggest(studentId, questionId, picked);
      setPicked("");
      onDone("机器建议已生成，必须由老师确认后才会计分");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const decide = async (answer: AnswerDetail, correct: boolean) => {
    const nextNote = (notes[answer.id] ?? answer.human_note ?? "").trim();
    const sameResult = answer.status === "confirmed" && answer.human_correct === correct;
    const sameNote = (answer.human_note ?? "") === nextNote;
    setBusy(true);
    try {
      await examAnswerHumanDecide(answer.id, correct, nextNote || null);
      if (sameResult && sameNote) {
        onDone("结论未变化，未重复累计错题或掌握度");
      } else if (sameResult) {
        onDone("终审备注已更新，结论和派生统计未变化");
      } else {
        onDone(answer.status === "confirmed" ? "改判已完成，错题与掌握度已重算" : "老师终审已保存");
      }
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="exam-grid">
        <section className="exam-card">
          <div className="exam-card-head">
            <div>
              <b>录入一题作答</b>
              <div className="meta">机器结果只进入待终审队列</div>
            </div>
            <span className={pending ? "tag wait" : "tag pass"}>{pending} 条待确认</span>
          </div>
          {students.length === 0 || objective.length === 0 ? (
            <div className="empty-state">
              {students.length === 0 ? "请先在“学生”中导入学生。" : "请先在“题库”中录入客观题。"}
            </div>
          ) : (
            <div className="form exam-form">
              <label className="field">
                <span className="fl">学生</span>
                <select value={studentId} onChange={(event) => setStudentId(Number(event.target.value))}>
                  {students.map((student) => <option key={student.id} value={student.id}>{student.name} · {student.student_no}</option>)}
                </select>
              </label>
              <label className="field">
                <span className="fl">题目</span>
                <select value={questionId} onChange={(event) => setQuestionId(Number(event.target.value))}>
                  {objective.map((question) => (
                    <option key={question.id} value={question.id}>
                      {question.question_no ?? `#${question.id}`} · {TYPE_LABEL[question.qtype]} · {question.stem.slice(0, 28)}
                    </option>
                  ))}
                </select>
              </label>
              {selectedQuestion && (
                <div className="answer-reference">
                  <span>标准答案</span><b>{selectedQuestion.correct_answer || "未设置"}</b>
                  <span>分值</span><b>{selectedQuestion.max_score}</b>
                </div>
              )}
              <label className="field">
                <span className="fl">学生答案</span>
                <input
                  type="text"
                  value={picked}
                  placeholder={selectedQuestion?.qtype === "judge" ? "如：对 / 错" : "如：B / AC / 填空内容"}
                  onChange={(event) => setPicked(event.target.value)}
                  onKeyDown={(event) => event.key === "Enter" && suggest()}
                />
              </label>
              <button className="primary" disabled={busy} onClick={suggest}>
                {busy ? "处理中…" : "生成机器建议"}
              </button>
            </div>
          )}
        </section>
        <section className="exam-card principle-card">
          <div className="exam-card-head"><b>判定规则</b></div>
          <ol>
            <li>单选、多选、判断和填空只做确定性标准化对比。</li>
            <li>机器建议不写错题本，也不改变知识点掌握度。</li>
            <li>老师可以确认或改判；每次改判都会重算派生结果。</li>
            <li>主观题继续保留人工判定，不交给这条自动链路。</li>
          </ol>
        </section>
      </div>

      <div className="sech">终审队列 <span className="n">最近 {answers.length} 条</span></div>
      {answers.length === 0 && <div className="empty-state">暂无作答记录。</div>}
      {answers.map((answer) => (
        <div className={answer.status === "pending_review" ? "answer-row pending" : "answer-row"} key={answer.id}>
          <div className="answer-summary">
            <div className="answer-title">
              <b>{answer.student_name}</b>
              <span>{answer.question_no ?? `题目 #${answer.question_id}`}</span>
              <span className={answer.status === "pending_review" ? "tag wait" : "tag pass"}>
                {answer.status === "pending_review" ? "待老师确认" : "已终审"}
              </span>
            </div>
            <div className="answer-stem">{answer.question_stem}</div>
            <div className="answer-facts">
              <span>学生答案 <b>{answer.picked || "—"}</b></span>
              <span>标准答案 <b>{answer.correct_answer || "—"}</b></span>
              <span>机器建议 <b className={answer.machine_correct == null ? "" : answer.machine_correct ? "ok-text" : "bad-text"}>{answer.machine_correct == null ? "无" : answer.machine_correct ? "正确" : "错误"}</b></span>
              <span>老师结论 <b>{answer.human_correct == null ? "未确认" : answer.human_correct ? "正确" : "错误"}</b></span>
              {answer.knowledge_point_name && <span>命中知识点 <b>{answer.knowledge_point_name}</b></span>}
            </div>
            <div className="meta">{displayTime(answer.created_at)} · {answer.machine_note}</div>
          </div>
          <div className="answer-review">
            <input
              type="text"
              placeholder="终审备注（可选）"
              value={notes[answer.id] ?? answer.human_note ?? ""}
              onChange={(event) => setNotes((current) => ({ ...current, [answer.id]: event.target.value }))}
            />
            <div>
              <button disabled={busy} onClick={() => decide(answer, false)}>判为错误</button>
              <button className="primary" disabled={busy} onClick={() => decide(answer, true)}>判为正确</button>
            </div>
          </div>
        </div>
      ))}
    </>
  );
}

function QuestionTab({
  questions,
  knowledge,
  onDone,
  onError,
}: {
  questions: Question[];
  knowledge: KnowledgePoint[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [questionNo, setQuestionNo] = useState("");
  const [qtype, setQtype] = useState<Question["qtype"]>("single");
  const [stem, setStem] = useState("");
  const [correct, setCorrect] = useState("");
  const [maxScore, setMaxScore] = useState(1);
  const [kpId, setKpId] = useState(0);
  const [optionsText, setOptionsText] = useState("");
  const [busy, setBusy] = useState(false);

  const save = async () => {
    if (!stem.trim() || !correct.trim()) {
      onError("题干和标准答案不能为空");
      return;
    }
    const normalizedCorrect = correct.toUpperCase().replace(/[^A-Z0-9]/g, "");
    const options = optionsText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line, ord) => {
        const matched = line.match(/^([A-Za-z])[\.、:：)）\s-]*(.+)$/);
        const label = matched?.[1].toUpperCase() ?? String.fromCharCode(65 + ord);
        return {
          label,
          content: matched?.[2].trim() ?? line,
          is_correct: normalizedCorrect.includes(label),
          knowledge_point_id: kpId || null,
          analysis: null,
          ord,
        };
      });
    const input: QuestionInput = {
      subject_id: null,
      question_no: questionNo.trim() || null,
      qtype,
      stem: stem.trim(),
      image_path: null,
      correct_answer: correct.trim(),
      knowledge_point_id: kpId || null,
      difficulty: null,
      analysis: null,
      max_score: maxScore,
      enabled: true,
      options,
    };
    setBusy(true);
    try {
      await questionCreate(input);
      setQuestionNo("");
      setStem("");
      setCorrect("");
      setOptionsText("");
      onDone("题目已保存，可立即进入快速批改");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exam-grid wide-left">
      <section className="exam-card">
        <div className="exam-card-head"><div><b>手工录题</b><div className="meta">OCR 接入前的可用兜底入口</div></div></div>
        <div className="form exam-form">
          <div className="form-pair">
            <label className="field"><span className="fl">题号（可选，需唯一）</span><input type="text" value={questionNo} onChange={(event) => setQuestionNo(event.target.value)} placeholder="如 PHY-001" /></label>
            <label className="field"><span className="fl">题型</span><select value={qtype} onChange={(event) => setQtype(event.target.value as Question["qtype"])}><option value="single">单选</option><option value="multi">多选</option><option value="judge">判断</option><option value="fill">填空</option></select></label>
          </div>
          <label className="field"><span className="fl">题干</span><textarea rows={4} value={stem} onChange={(event) => setStem(event.target.value)} /></label>
          <div className="form-pair">
            <label className="field"><span className="fl">标准答案</span><input type="text" value={correct} onChange={(event) => setCorrect(event.target.value)} placeholder="如 B / AC / 对" /></label>
            <label className="field"><span className="fl">分值</span><input type="number" min="0.5" step="0.5" value={maxScore} onChange={(event) => setMaxScore(Number(event.target.value))} /></label>
          </div>
          <label className="field"><span className="fl">主知识点</span><select value={kpId} onChange={(event) => setKpId(Number(event.target.value))}><option value={0}>未设置</option>{knowledge.map((kp) => <option value={kp.id} key={kp.id}>{kp.code ? `${kp.code} · ` : ""}{kp.name}</option>)}</select></label>
          {(qtype === "single" || qtype === "multi") && <label className="field"><span className="fl">选项（每行一项，可选）</span><textarea rows={5} value={optionsText} onChange={(event) => setOptionsText(event.target.value)} placeholder={"A. 选项内容\nB. 选项内容"} /></label>}
          <button className="primary" disabled={busy} onClick={save}>{busy ? "保存中…" : "保存题目"}</button>
        </div>
      </section>
      <section className="exam-card question-list-card">
        <div className="exam-card-head"><b>题库</b><span className="tag">{questions.length} 题</span></div>
        {questions.length === 0 && <div className="empty-state">还没有题目。</div>}
        {questions.map((question) => (
          <div className="question-item" key={question.id}>
            <div><span className="cno">{question.question_no ?? `#${question.id}`}</span><span className="tag">{TYPE_LABEL[question.qtype]}</span></div>
            <b>{question.stem}</b>
            <div className="meta">答案 {question.correct_answer || "—"} · {question.max_score} 分 · {knowledge.find((kp) => kp.id === question.knowledge_point_id)?.name ?? "未关联知识点"}</div>
          </div>
        ))}
      </section>
    </div>
  );
}

function KnowledgeTab({
  knowledge,
  onDone,
  onError,
}: {
  knowledge: KnowledgePoint[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [parentId, setParentId] = useState(0);
  const [busy, setBusy] = useState(false);
  const roots = useMemo(() => knowledge.filter((kp) => kp.parent_id == null), [knowledge]);

  const save = async () => {
    if (!name.trim()) {
      onError("知识点名称不能为空");
      return;
    }
    setBusy(true);
    try {
      await kpCreate(name.trim(), code.trim() || null, parentId || null);
      setName("");
      setCode("");
      onDone("知识点已创建");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exam-grid">
      <section className="exam-card">
        <div className="exam-card-head"><b>新增知识点</b></div>
        <div className="form exam-form">
          <label className="field"><span className="fl">名称</span><input type="text" value={name} onChange={(event) => setName(event.target.value)} placeholder="如 动量守恒" /></label>
          <label className="field"><span className="fl">编码（可选）</span><input type="text" value={code} onChange={(event) => setCode(event.target.value)} placeholder="如 PHY-MOM" /></label>
          <label className="field"><span className="fl">上级板块（可选）</span><select value={parentId} onChange={(event) => setParentId(Number(event.target.value))}><option value={0}>根节点</option>{roots.map((kp) => <option key={kp.id} value={kp.id}>{kp.name}</option>)}</select></label>
          <button className="primary" disabled={busy} onClick={save}>{busy ? "保存中…" : "创建知识点"}</button>
        </div>
      </section>
      <section className="exam-card">
        <div className="exam-card-head"><b>知识点树</b><span className="tag">{knowledge.length} 个</span></div>
        {roots.length === 0 && <div className="empty-state">还没有知识点。</div>}
        {roots.map((root) => (
          <div className="kp-group" key={root.id}>
            <div className="kp-root"><span>◆</span><b>{root.name}</b>{root.code && <code>{root.code}</code>}</div>
            {knowledge.filter((kp) => kp.parent_id === root.id).map((child) => <div className="kp-child" key={child.id}><span>└</span>{child.name}{child.code && <code>{child.code}</code>}</div>)}
          </div>
        ))}
      </section>
    </div>
  );
}
