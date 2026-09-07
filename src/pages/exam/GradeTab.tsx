import { useState } from "react";
import { examAnswerHumanDecide, examAnswerSuggest } from "../../api/exam";
import type { AnswerDetail, Question } from "../../api/exam";
import type { Student } from "../../api/manage";
import { displayTime } from "./examPure";
import { TYPE_LABEL } from "./questionTypes";

export function GradeTab({
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
