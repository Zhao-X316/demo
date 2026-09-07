import { useState, useMemo, useEffect } from "react";
import {
  loadAnswerTargets,
  AnswerSourceInboxItem,
  AnswerMatchDraft,
  loadAnswerInbox,
  importAndAnalyzeAnswers,
  K1AnswerPayload,
  FillAnswerSlot,
  ShortAnswerRubricPoint,
  confirmAnswerMatch,
} from "../../api/knowledge";
import { open } from "@tauri-apps/plugin-dialog";
import { sourceFileLabel, TYPE_LABEL, sameNumber } from "./shared";

function newAnswerRequestKey(action: "import" | "confirm") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-answer-${action}-${random}`;
}

type FillEditorRow = {
  answers: string;
  variants: string;
  maxScore: number;
};

type RubricEditorRow = {
  text: string;
  maxScore: number;
  paraphrases: string;
  concepts: string;
};

function stringValues(value: unknown) {
  return Array.isArray(value)
    ? value.filter((entry): entry is string => typeof entry === "string")
    : [];
}

export function AnswerSourcePanel() {
  const [targets, setTargets] = useState<Awaited<ReturnType<typeof loadAnswerTargets>>>([]);
  const [targetId, setTargetId] = useState("");
  const [path, setPath] = useState("");
  const [items, setItems] = useState<AnswerSourceInboxItem[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [labels, setLabels] = useState("");
  const [trueFalse, setTrueFalse] = useState<"" | "true" | "false">("");
  const [fillRows, setFillRows] = useState<FillEditorRow[]>([]);
  const [referenceAnswer, setReferenceAnswer] = useState("");
  const [rubricRows, setRubricRows] = useState<RubricEditorRow[]>([]);
  const [note, setNote] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const pendingDrafts = useMemo(
    () => items.flatMap((item) => (
      item.latestExtraction?.drafts
        .filter((draft) => !draft.reviewed)
        .map((draft) => ({ draft, item })) ?? []
    )),
    [items],
  );
  const selectedEntry = pendingDrafts.find(({ draft }) => draft.publicId === selectedId) ?? null;
  const selected = selectedEntry?.draft ?? null;

  const populate = (draft: AnswerMatchDraft | null) => {
    setSelectedId(draft?.publicId ?? "");
    setLabels("");
    setTrueFalse("");
    setFillRows([]);
    setReferenceAnswer("");
    setRubricRows([]);
    setNote("");
    if (!draft?.answerJson) {
      if (draft?.questionType === "fill_blank") {
        setFillRows([{ answers: "", variants: "", maxScore: draft.maxScore }]);
      }
      return;
    }
    const answer = draft.answerJson as Record<string, unknown>;
    if (draft.questionType === "single" || draft.questionType === "multiple") {
      setLabels(stringValues(answer.correct_labels).join(", "));
    } else if (draft.questionType === "true_false") {
      setTrueFalse(typeof answer.correct === "boolean" ? String(answer.correct) as "true" | "false" : "");
    } else if (draft.questionType === "fill_blank") {
      const slots = Array.isArray(answer.slots) ? answer.slots : [];
      setFillRows(slots.map((slot, index) => {
        const value = slot as Record<string, unknown>;
        return {
          answers: stringValues(value.canonical_answers).join(" | "),
          variants: stringValues(value.accepted_variants).join(" | "),
          maxScore: typeof value.max_score === "number"
            ? value.max_score
            : draft.maxScore / Math.max(slots.length, 1),
          orderIndex: index + 1,
        };
      }));
    } else {
      setReferenceAnswer(typeof answer.reference_answer === "string" ? answer.reference_answer : "");
      const points = Array.isArray(answer.rubric_points) ? answer.rubric_points : [];
      setRubricRows(points.map((point) => {
        const value = point as Record<string, unknown>;
        return {
          text: typeof value.canonical_text === "string" ? value.canonical_text : "",
          maxScore: typeof value.max_score === "number" ? value.max_score : 0,
          paraphrases: stringValues(value.allowed_paraphrases).join("，"),
          concepts: stringValues(value.required_concepts).join("，"),
        };
      }));
    }
  };

  const refresh = async (preferredId?: string) => {
    const [nextTargets, nextItems] = await Promise.all([
      loadAnswerTargets(50),
      loadAnswerInbox(50),
    ]);
    setTargets(nextTargets);
    setItems(nextItems);
    setTargetId((current) => (
      nextTargets.some((target) => (
        target.questionSourceDocumentPublicId === current && target.pendingCount > 0
      ))
        ? current
        : nextTargets.find((target) => target.pendingCount > 0)?.questionSourceDocumentPublicId ?? ""
    ));
    const drafts = nextItems.flatMap((item) => (
      item.latestExtraction?.drafts.filter((draft) => !draft.reviewed) ?? []
    ));
    populate(drafts.find((draft) => draft.publicId === preferredId) ?? drafts[0] ?? null);
  };

  useEffect(() => {
    refresh().catch((reason) => setError(String(reason)));
  }, []);

  const chooseFile = async () => {
    const selectedPath = await open({
      multiple: false,
      directory: false,
      filters: [{
        name: "答案资料",
        extensions: ["jpg", "jpeg", "pdf", "txt", "docx", "xlsx"],
      }],
    });
    if (typeof selectedPath === "string") {
      setPath(selectedPath);
      setError("");
      setNotice("");
    }
  };

  const analyze = async () => {
    if (!path || !targetId) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const result = await importAndAnalyzeAnswers(
        path,
        targetId,
        newAnswerRequestKey("import"),
      );
      await refresh(result.extraction?.drafts[0]?.publicId);
      if (result.status === "failed") {
        setError(result.failure?.safeMessage ?? "答案识别失败，文件已保留，可稍后重试。");
      } else {
        const matched = result.extraction?.matchedCount ?? 0;
        const total = result.extraction?.targetCount ?? 0;
        setNotice(`已匹配 ${matched}/${total} 道。清晰答案可整批确认，分歧和缺失项在下方补录。`);
        setPath("");
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const payload = (draft: AnswerMatchDraft): K1AnswerPayload | null => {
    if (draft.questionType === "single" || draft.questionType === "multiple") {
      const correctLabels = labels.split(/[,，\s]+/).map((value) => value.trim()).filter(Boolean);
      return correctLabels.length
        ? { schema_version: 1, correct_labels: correctLabels }
        : null;
    }
    if (draft.questionType === "true_false") {
      return trueFalse
        ? { schema_version: 1, correct: trueFalse === "true" }
        : null;
    }
    if (draft.questionType === "fill_blank") {
      const slots: FillAnswerSlot[] = fillRows.map((row, index) => ({
        order_index: index + 1,
        canonical_answers: row.answers.split("|").map((value) => value.trim()).filter(Boolean),
        accepted_variants: row.variants.split("|").map((value) => value.trim()).filter(Boolean),
        max_score: row.maxScore,
      }));
      return slots.length && slots.every((slot) => slot.canonical_answers.length && slot.max_score > 0)
        ? { schema_version: 1, slots }
        : null;
    }
    const points: ShortAnswerRubricPoint[] = rubricRows.map((row, index) => ({
      order_index: index + 1,
      canonical_text: row.text.trim(),
      max_score: row.maxScore,
      allowed_paraphrases: row.paraphrases.split(/[，,]/).map((value) => value.trim()).filter(Boolean),
      required_concepts: row.concepts.split(/[，,]/).map((value) => value.trim()).filter(Boolean),
    }));
    if (!referenceAnswer.trim()) return null;
    if (points.some((point) => !point.canonical_text || point.max_score <= 0)) return null;
    return points.length
      ? { schema_version: 1, reference_answer: referenceAnswer.trim(), rubric_points: points }
      : { schema_version: 1, reference_answer: referenceAnswer.trim() };
  };

  const confirmOne = async () => {
    if (!selected) return;
    const answer = payload(selected);
    if (!answer) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const review = await confirmAnswerMatch(
        selected.publicId,
        selected.contentHash,
        answer,
        note.trim() || null,
        newAnswerRequestKey("confirm"),
      );
      await refresh();
      window.dispatchEvent(new Event("k1-answer-confirmed"));
      setNotice(
        review.resultQuality === "L2"
          ? "答案和批改规则已确认，这道题现在可用于自动批改。"
          : "参考答案已确认，这道简答题现在可练习；补齐评分点后可用于自动批改。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const confirmReadyBatch = async (item: AnswerSourceInboxItem) => {
    const drafts = item.latestExtraction?.drafts.filter(
      (draft) => !draft.reviewed && draft.candidateState === "ready" && draft.answerJson,
    ) ?? [];
    if (!drafts.length) return;
    setWorking(true);
    setError("");
    setNotice("");
    let confirmed = 0;
    try {
      for (const draft of drafts) {
        await confirmAnswerMatch(
          draft.publicId,
          draft.contentHash,
          draft.answerJson!,
          "老师批量确认本次高置信度答案匹配",
          newAnswerRequestKey("confirm"),
        );
        confirmed += 1;
      }
      await refresh();
      window.dispatchEvent(new Event("k1-answer-confirmed"));
      setNotice(`已确认 ${confirmed} 道高置信度答案；分歧和缺失项仍保留待处理。`);
    } catch (reason) {
      await refresh();
      setError(`已确认 ${confirmed} 道，后续项目停止：${String(reason)}`);
    } finally {
      setWorking(false);
    }
  };

  const currentPayload = selected ? payload(selected) : null;
  const scoreTotal = selected?.questionType === "fill_blank"
    ? fillRows.reduce((total, row) => total + Number(row.maxScore || 0), 0)
    : rubricRows.reduce((total, row) => total + Number(row.maxScore || 0), 0);

  return (
    <>
      {error && <div className="error">{error}</div>}
      {notice && <div className="ok-banner">{notice}</div>}
      <section className="dashboard-panel answer-source-hero">
        <div className="dashboard-panel-head">
          <div>
            <b>给刚导入的题目补答案</b>
            <span>上传答案照片、PDF、Word、Excel 或 TXT；系统自动按题号匹配，缺失项不会跳过</span>
          </div>
          <span className="tag">老师确认后才生效</span>
        </div>
        <div className="answer-source-upload">
          <label className="field">
            <span className="fl">选择题面批次</span>
            <select value={targetId} onChange={(event) => setTargetId(event.target.value)}>
              {!targets.some((target) => target.pendingCount > 0) && <option value="">暂无待补答案题目</option>}
              {targets.filter((target) => target.pendingCount > 0).map((target) => (
                <option
                  key={target.questionSourceDocumentPublicId}
                  value={target.questionSourceDocumentPublicId}
                >
                  {target.sourceFormat.toUpperCase()} · 待补 {target.pendingCount}/{target.targetCount} 道
                </option>
              ))}
            </select>
          </label>
          <button onClick={chooseFile} disabled={working || !targetId}>
            {path ? "重新选择答案" : "选择答案文件"}
          </button>
          <div className="answer-file-name">
            <b>{path ? sourceFileLabel(path) : "还没有选择答案文件"}</b>
            <span>答案只来自老师上传资料；系统不会上网搜索或按学生多数作答猜答案</span>
          </div>
          <button className="primary" disabled={working || !path || !targetId} onClick={analyze}>
            {working ? "正在匹配…" : "识别并匹配答案"}
          </button>
        </div>
      </section>

      {items.length > 0 && (
        <section className="dashboard-panel source-inbox-summary">
          <div className="dashboard-panel-head">
            <div>
              <b>最近答案匹配</b>
              <span>清晰项可整批确认；任何缺题、低置信度或分值不一致都留给老师</span>
            </div>
            <span className={pendingDrafts.length ? "tag warn" : "tag ok"}>
              {pendingDrafts.length} 道待处理
            </span>
          </div>
          <div className="source-document-list">
            {items.map((item) => {
              const extraction = item.latestExtraction;
              const failed = item.latestAiStatus === "failed";
              const ready = extraction?.drafts.filter(
                (draft) => !draft.reviewed && draft.candidateState === "ready" && draft.answerJson,
              ).length ?? 0;
              return (
                <div key={item.document.publicId} className="source-document-row">
                  <div>
                    <b>答案资料 · {item.document.sourceFormat.toUpperCase()}</b>
                    <span>
                      {extraction
                        ? `匹配 ${extraction.matchedCount}/${extraction.targetCount} 道 · 待处理 ${item.pendingMatches} 道`
                        : failed
                          ? "识别失败，来源已保留；点击上方按钮可直接重试"
                          : "等待识别"}
                    </span>
                  </div>
                  {ready > 0 ? (
                    <button className="primary" disabled={working} onClick={() => confirmReadyBatch(item)}>
                      确认 {ready} 道清晰答案
                    </button>
                  ) : failed ? (
                    <span className="tag warn">识别失败</span>
                  ) : (
                    <span className={item.pendingMatches ? "tag warn" : "tag ok"}>
                      {item.pendingMatches ? "需要逐题核对" : "已处理"}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}

      {pendingDrafts.length > 0 && (
        <div className="source-review-layout">
          <aside className="dashboard-panel source-review-list">
            {pendingDrafts.map(({ draft }) => (
              <button
                key={draft.publicId}
                className={draft.publicId === selectedId ? "active" : ""}
                onClick={() => populate(draft)}
              >
                <span>
                  第 {draft.questionNo} 题 · {TYPE_LABEL[draft.questionType]} ·
                  {" "}
                  {draft.candidateState === "missing"
                    ? "答案缺失"
                    : `${Math.round(draft.confidence * 100)}%`}
                </span>
                <b>{draft.stem}</b>
                <small>
                  {draft.candidateState === "ready"
                    ? "可直接确认"
                    : draft.candidateState === "missing"
                      ? "请补录答案"
                      : "请核对分歧"}
                </small>
              </button>
            ))}
          </aside>
          {selected && (
            <section className="dashboard-panel source-review-editor answer-review-editor">
              <div className="dashboard-panel-head">
                <div>
                  <b>第 {selected.questionNo} 题 · {TYPE_LABEL[selected.questionType]}</b>
                  <span>{selected.stem}</span>
                </div>
                <span className={selected.candidateState === "ready" ? "tag ok" : "tag warn"}>
                  {selected.candidateState === "missing" ? "答案缺失" : "老师终审"}
                </span>
              </div>

              {(selected.questionType === "single" || selected.questionType === "multiple") && (
                <label className="field">
                  <span className="fl">正确选项（多选用逗号分隔）</span>
                  <input value={labels} placeholder="例如 A 或 A,C" onChange={(event) => setLabels(event.target.value)} />
                </label>
              )}
              {selected.questionType === "true_false" && (
                <label className="field">
                  <span className="fl">正确答案</span>
                  <select value={trueFalse} onChange={(event) => setTrueFalse(event.target.value as "" | "true" | "false")}>
                    <option value="">请选择</option>
                    <option value="true">正确</option>
                    <option value="false">错误</option>
                  </select>
                </label>
              )}
              {selected.questionType === "fill_blank" && (
                <div className="answer-rule-list">
                  <div className="answer-rule-head">
                    <span className="fl">每个空的答案与分值</span>
                    <span className={sameNumber(scoreTotal, selected.maxScore) ? "tag ok" : "tag warn"}>
                      {scoreTotal}/{selected.maxScore} 分
                    </span>
                  </div>
                  {fillRows.map((row, index) => (
                    <div className="answer-rule-row" key={`fill-${index}`}>
                      <b>空 {index + 1}</b>
                      <input
                        aria-label={`填空 ${index + 1} 标准答案`}
                        value={row.answers}
                        placeholder="标准答案；多个写法用 | 分隔"
                        onChange={(event) => setFillRows((current) => current.map(
                          (value, currentIndex) => currentIndex === index
                            ? { ...value, answers: event.target.value }
                            : value,
                        ))}
                      />
                      <input
                        aria-label={`填空 ${index + 1} 可接受写法`}
                        value={row.variants}
                        placeholder="其他可接受写法（可空）"
                        onChange={(event) => setFillRows((current) => current.map(
                          (value, currentIndex) => currentIndex === index
                            ? { ...value, variants: event.target.value }
                            : value,
                        ))}
                      />
                      <input
                        aria-label={`填空 ${index + 1} 分值`}
                        type="number"
                        min={0.001}
                        step={0.5}
                        value={row.maxScore}
                        onChange={(event) => setFillRows((current) => current.map(
                          (value, currentIndex) => currentIndex === index
                            ? { ...value, maxScore: Number(event.target.value) }
                            : value,
                        ))}
                      />
                      <button
                        disabled={fillRows.length === 1}
                        onClick={() => setFillRows((current) => current.filter((_, currentIndex) => currentIndex !== index))}
                      >
                        删除
                      </button>
                    </div>
                  ))}
                  <button onClick={() => setFillRows((current) => [
                    ...current,
                    { answers: "", variants: "", maxScore: 0 },
                  ])}>
                    增加一个空
                  </button>
                </div>
              )}
              {selected.questionType === "short_answer" && (
                <>
                  <label className="field">
                    <span className="fl">参考答案</span>
                    <textarea rows={4} value={referenceAnswer} onChange={(event) => setReferenceAnswer(event.target.value)} />
                  </label>
                  <div className="answer-rule-list">
                    <div className="answer-rule-head">
                      <div>
                        <span className="fl">评分点（可稍后补）</span>
                        <small>不填评分点时先保存为 L1 可练习；分值完整后为 L2 可自动批改</small>
                      </div>
                      {rubricRows.length > 0 && (
                        <span className={sameNumber(scoreTotal, selected.maxScore) ? "tag ok" : "tag warn"}>
                          {scoreTotal}/{selected.maxScore} 分
                        </span>
                      )}
                    </div>
                    {rubricRows.map((row, index) => (
                      <div className="answer-rule-row rubric-row" key={`rubric-${index}`}>
                        <b>点 {index + 1}</b>
                        <input
                          aria-label={`评分点 ${index + 1} 内容`}
                          value={row.text}
                          placeholder="得分要点"
                          onChange={(event) => setRubricRows((current) => current.map(
                            (value, currentIndex) => currentIndex === index
                              ? { ...value, text: event.target.value }
                              : value,
                          ))}
                        />
                        <input
                          aria-label={`评分点 ${index + 1} 分值`}
                          type="number"
                          min={0.001}
                          step={0.5}
                          value={row.maxScore}
                          onChange={(event) => setRubricRows((current) => current.map(
                            (value, currentIndex) => currentIndex === index
                              ? { ...value, maxScore: Number(event.target.value) }
                              : value,
                          ))}
                        />
                        <input
                          aria-label={`评分点 ${index + 1} 允许表达`}
                          value={row.paraphrases}
                          placeholder="允许表达（逗号分隔）"
                          onChange={(event) => setRubricRows((current) => current.map(
                            (value, currentIndex) => currentIndex === index
                              ? { ...value, paraphrases: event.target.value }
                              : value,
                          ))}
                        />
                        <input
                          aria-label={`评分点 ${index + 1} 必需概念`}
                          value={row.concepts}
                          placeholder="必需概念（逗号分隔）"
                          onChange={(event) => setRubricRows((current) => current.map(
                            (value, currentIndex) => currentIndex === index
                              ? { ...value, concepts: event.target.value }
                              : value,
                          ))}
                        />
                        <button onClick={() => setRubricRows((current) => current.filter((_, currentIndex) => currentIndex !== index))}>
                          删除
                        </button>
                      </div>
                    ))}
                    <button onClick={() => setRubricRows((current) => [
                      ...current,
                      { text: "", maxScore: 0, paraphrases: "", concepts: "" },
                    ])}>
                      增加评分点
                    </button>
                  </div>
                </>
              )}
              {selected.sourceAnchor && (
                <div className="source-answer-anchor">
                  来源定位：{JSON.stringify(selected.sourceAnchor)}
                </div>
              )}
              <label className="field">
                <span className="fl">老师备注（可选）</span>
                <input value={note} maxLength={300} onChange={(event) => setNote(event.target.value)} />
              </label>
              <div className="candidate-review-actions">
                <div>
                  <b>
                    {selected.questionType === "short_answer" && rubricRows.length === 0
                      ? "确认后可练习（L1）"
                      : "确认后可自动批改（L2）"}
                  </b>
                  <span>这一步只建立题库答案，不会创建作业、改写历史成绩或生成图谱证据。</span>
                </div>
                <button
                  className="primary"
                  disabled={working || !currentPayload
                    || ((selected.questionType === "fill_blank" || rubricRows.length > 0)
                      && !sameNumber(scoreTotal, selected.maxScore))}
                  onClick={confirmOne}
                >
                  {working ? "正在确认…" : "确认这道答案"}
                </button>
              </div>
            </section>
          )}
        </div>
      )}
    </>
  );
}
