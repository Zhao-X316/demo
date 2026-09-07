import { useState } from "react";
import { questionCreate } from "../../api/exam";
import type { KnowledgePoint, Question, QuestionInput } from "../../api/exam";
import { TYPE_LABEL } from "./questionTypes";

export function QuestionTab({
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
