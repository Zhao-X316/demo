import {
  CandidateReviewItem,
  K1QuestionType,
  loadCandidateReviewInbox,
  promoteCandidateToL1,
  discardCandidate,
} from "../../api/knowledge";
import { useState, useEffect } from "react";
import { TYPE_LABEL, TYPE_ORDER } from "./shared";

function newCandidateRequestKey(action: "promote" | "discard") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-candidate-${action}-${random}`;
}

function candidateSourceLabel(source: CandidateReviewItem["source_type"]) {
  if (source === "blank_paper") return "空白卷";
  if (source === "source_document") return "电子文档";
  if (source === "student_paper") return "学生卷（仅脱敏文本）";
  return "作业本";
}

function answerHint(type: K1QuestionType) {
  if (type === "single") return "填写一个正确选项字母，如 B";
  if (type === "multiple") return "填写正确选项字母，如 A,C";
  if (type === "true_false") return "填写“对”或“错”";
  if (type === "fill_blank") return "填写标准答案；多个空的结构化将在 L2 整理";
  return "填写参考答案；评分点将在 L2 整理";
}

export function CandidateReviewPanel() {
  const [items, setItems] = useState<CandidateReviewItem[]>([]);
  const [pendingCount, setPendingCount] = useState(0);
  const [boundary, setBoundary] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [questionType, setQuestionType] = useState<K1QuestionType>("single");
  const [stem, setStem] = useState("");
  const [materialText, setMaterialText] = useState("");
  const [maxScore, setMaxScore] = useState(1);
  const [options, setOptions] = useState<CandidateReviewItem["options"]>([]);
  const [answerText, setAnswerText] = useState("");
  const [note, setNote] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const selected = items.find((item) => item.candidate_public_id === selectedId) ?? null;

  const populate = (item: CandidateReviewItem | null) => {
    setSelectedId(item?.candidate_public_id ?? "");
    setQuestionType(item?.question_type ?? "single");
    setStem(item?.stem ?? "");
    setMaterialText(item?.material_text ?? "");
    setMaxScore(item?.max_score ?? 1);
    setOptions(item?.options.map((option) => ({ ...option })) ?? []);
    setAnswerText("");
    setNote("");
    setError("");
    setNotice("");
  };

  const refresh = async (preferredId?: string) => {
    const inbox = await loadCandidateReviewInbox(false, 100);
    setItems(inbox.items);
    setPendingCount(inbox.pending_count);
    setBoundary(inbox.boundary_note);
    const next = inbox.items.find((item) => item.candidate_public_id === preferredId)
      ?? inbox.items[0]
      ?? null;
    populate(next);
  };

  useEffect(() => {
    refresh().catch((reason) => setError(String(reason)));
  }, []);

  const promote = async () => {
    if (!selected) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const decision = await promoteCandidateToL1({
        requestKey: newCandidateRequestKey("promote"),
        candidatePublicId: selected.candidate_public_id,
        expectedContentHash: selected.content_hash,
        questionType,
        stem: stem.trim(),
        materialText: materialText.trim() || null,
        maxScore,
        options: questionType === "single" || questionType === "multiple"
          ? options.map((option, index) => ({
            label: option.label.trim(),
            content: option.content.trim(),
            orderIndex: index,
          }))
          : [],
        answerText: answerText.trim(),
        note: note.trim() || null,
        reviewedBy: "local_teacher",
      });
      await refresh();
      setNotice(`已收入个人题库 ${decision.result_quality_level ?? ""}；当前作业和历史成绩未切换。`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const discard = async () => {
    if (!selected) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      await discardCandidate({
        requestKey: newCandidateRequestKey("discard"),
        candidatePublicId: selected.candidate_public_id,
        expectedContentHash: selected.content_hash,
        note: note.trim() || null,
        reviewedBy: "local_teacher",
      });
      await refresh();
      setNotice("已丢弃该候选；原批改证据仍保留，题目不会进入可复用题库。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  return (
    <>
      {error && <div className="error">{error}</div>}
      {notice && <div className="ok-banner">{notice}</div>}
      <section className="dashboard-panel candidate-review-overview">
        <div className="dashboard-panel-head">
          <div><b>待整理题目候选</b><span>{boundary || "批改解析形成候选后，只需要处理系统无法自行确认的新题"}</span></div>
          <span className={pendingCount > 0 ? "tag warn" : "tag ok"}>{pendingCount} 道待整理</span>
        </div>
      </section>
      {!items.length ? (
        <div className="dashboard-panel muted candidate-empty">
          当前没有待整理题目。候选整理内核已就绪；题面提取入口接通后，无法精确复用的新题会进入这里。
        </div>
      ) : (
        <div className="candidate-review-layout">
          <aside className="dashboard-panel candidate-review-list">
            {items.map((item) => (
              <button
                key={item.candidate_public_id}
                className={item.candidate_public_id === selectedId ? "candidate-list-item active" : "candidate-list-item"}
                onClick={() => populate(item)}
              >
                <span>{TYPE_LABEL[item.question_type]} · {candidateSourceLabel(item.source_type)}</span>
                <b>{item.stem}</b>
                <small>{item.privacy_status === "text_only" ? "仅脱敏文字" : "含干净题面来源"}</small>
              </button>
            ))}
          </aside>
          {selected && (
            <section className="dashboard-panel candidate-review-editor">
              <div className="dashboard-panel-head">
                <div><b>核对后收入我的题库</b><span>系统不会替你确认答案，也不会自动进入学习图谱</span></div>
                <span className="tag">C0 → L1</span>
              </div>
              <div className="candidate-editor-grid">
                <label className="field">
                  <span className="fl">题型</span>
                  <select value={questionType} onChange={(event) => setQuestionType(event.target.value as K1QuestionType)}>
                    {TYPE_ORDER.map((type) => <option value={type} key={type}>{TYPE_LABEL[type]}</option>)}
                  </select>
                </label>
                <label className="field">
                  <span className="fl">分值</span>
                  <input type="number" min={0.001} max={500} step={0.5} value={maxScore} onChange={(event) => setMaxScore(Number(event.target.value))} />
                </label>
              </div>
              <label className="field">
                <span className="fl">题干</span>
                <textarea rows={3} value={stem} onChange={(event) => setStem(event.target.value)} />
              </label>
              <label className="field">
                <span className="fl">材料（没有可留空）</span>
                <textarea rows={2} value={materialText} onChange={(event) => setMaterialText(event.target.value)} />
              </label>
              {questionType !== "single" && questionType !== "multiple" ? null : (
                <div className="candidate-option-editor">
                  <span className="fl">选项</span>
                  {options.map((option, index) => (
                    <div key={`${index}-${option.label}`}>
                      <input
                        aria-label={`选项 ${index + 1} 标签`}
                        value={option.label}
                        maxLength={8}
                        onChange={(event) => setOptions((current) => current.map(
                          (value, currentIndex) => currentIndex === index ? { ...value, label: event.target.value } : value,
                        ))}
                      />
                      <input
                        aria-label={`选项 ${index + 1} 内容`}
                        value={option.content}
                        onChange={(event) => setOptions((current) => current.map(
                          (value, currentIndex) => currentIndex === index ? { ...value, content: event.target.value } : value,
                        ))}
                      />
                      <button
                        aria-label={`删除选项 ${index + 1}`}
                        disabled={options.length <= 2}
                        onClick={() => setOptions((current) => current.filter(
                          (_, currentIndex) => currentIndex !== index,
                        ))}
                      >
                        删除
                      </button>
                    </div>
                  ))}
                  <button onClick={() => setOptions((current) => [
                    ...current,
                    { label: String.fromCharCode(65 + current.length), content: "", order_index: current.length },
                  ])}>增加选项</button>
                </div>
              )}
              <label className="field">
                <span className="fl">标准答案</span>
                <input value={answerText} placeholder={answerHint(questionType)} onChange={(event) => setAnswerText(event.target.value)} />
              </label>
              <label className="field">
                <span className="fl">备注（可选）</span>
                <input value={note} maxLength={500} placeholder="如：按本校课堂口径核对" onChange={(event) => setNote(event.target.value)} />
              </label>
              <div className="candidate-review-actions">
                <div>
                  <b>确认后仅达到“可练习（L1）”</b>
                  <span>填空槽位、简答评分点和知识链接还需后续确认。</span>
                </div>
                <button disabled={working} onClick={discard}>不收入题库</button>
                <button className="primary" disabled={working || !stem.trim() || !answerText.trim()} onClick={promote}>
                  {working ? "正在保存…" : "确认并收入我的题库"}
                </button>
              </div>
            </section>
          )}
        </div>
      )}
    </>
  );
}
