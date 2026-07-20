import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AcceptSourceDraftInput,
  AnswerMatchDraft,
  AnswerSourceInboxItem,
  FillAnswerSlot,
  K1AnswerPayload,
  ConfirmedSourceLinksInput,
  LinkReviewInboxItem,
  LinkSuggestionDraftView,
  LinkSuggestionInput,
  LinkSuggestionSource,
  ShortAnswerRubricPoint,
  BlueprintAssembly,
  BlueprintCandidate,
  BlueprintOptions,
  BlueprintPreview,
  BlueprintPreviewInput,
  BlueprintPaperEdition,
  BlueprintPaperEditor,
  BlueprintPaperEditorItem,
  CandidateReviewItem,
  DuplicateCandidate,
  K1QuestionType,
  QuestionSearchInput,
  QuestionSearchResponse,
  QuestionImpactAction,
  QuestionImpactPlan,
  QuestionImpactReviewCase,
  AssessmentDefaultUpgrade,
  QuestionPerformanceCatalog,
  QuestionPerformanceItem,
  QuestionVersionImpactPreview,
  SourceDraft,
  SourceInboxItem,
  acceptSourceDraft,
  confirmAnswerMatch,
  confirmKnowledgeLinks,
  confirmQuestionImpact,
  confirmBlueprint,
  confirmBlueprintPaper,
  discardSourceDraft,
  discardCandidate,
  importAndAnalyzeSource,
  importAndAnalyzeAnswers,
  listBlueprintAssemblies,
  loadBlueprintPaperEditor,
  loadCandidateReviewInbox,
  loadBlueprintOptions,
  loadSourceInbox,
  loadAnswerInbox,
  loadAnswerTargets,
  loadLinkReviewCatalog,
  loadLinkReviewEditor,
  loadLinkReviewInbox,
  previewBlueprint,
  previewQuestionImpact,
  promoteCandidateToL1,
  reviewDuplicate,
  searchQuestions,
  suggestKnowledgeLinks,
  loadQuestionPerformance,
  loadQuestionImpactCases,
  prepareQuestionImpactCases,
  publishQuestionImpactCase,
  resolveQuestionImpactCase,
  upgradeAssessmentDefaultFromImpact,
  writeBlueprintPaper,
} from "../api/knowledge";

const TYPE_LABEL: Record<K1QuestionType, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
  fill_blank: "填空",
  short_answer: "简答",
};

const TYPE_ORDER: K1QuestionType[] = [
  "single",
  "multiple",
  "true_false",
  "fill_blank",
  "short_answer",
];

function newRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-blueprint-${random}`;
}

function newPaperRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-blueprint-paper-${random}`;
}

function newDuplicateRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-duplicate-${random}`;
}

function newCandidateRequestKey(action: "promote" | "discard") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-candidate-${action}-${random}`;
}

function newSourceRequestKey(action: "import" | "accept" | "discard") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-source-${action}-${random}`;
}

function newAnswerRequestKey(action: "import" | "confirm") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-answer-${action}-${random}`;
}

function newLinkRequestKey(action: "suggest" | "confirm") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-link-${action}-${random}`;
}

function newImpactRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-impact-${random}`;
}

function sameNumber(left: number, right: number) {
  return Math.abs(left - right) < 0.000001;
}

function duplicateLabel(candidate: DuplicateCandidate) {
  if (candidate.decision === "same_family") return "老师已归为同题变式";
  if (candidate.decision === "independent") return "老师已确认是不同题";
  return candidate.match_kind === "exact" ? "内容完全一致" : "疑似同题变式";
}

function sourceFileLabel(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function sourceTypeLabel(source: SourceInboxItem["document"]["sourceType"]) {
  return source === "blank_paper" ? "空白试卷" : "电子题目文件";
}

function sourceFailureMessage(item: SourceInboxItem) {
  if (!item.latestAiErrorMetaJson) return "识别失败，来源已保留，可重新上传重试";
  try {
    const value = JSON.parse(item.latestAiErrorMetaJson) as {
      safeMessage?: unknown;
      safe_message?: unknown;
    };
    const safeMessage = value.safeMessage ?? value.safe_message;
    return typeof safeMessage === "string"
      ? safeMessage
      : "识别失败，来源已保留，可重新上传重试";
  } catch {
    return "识别失败，来源已保留，可重新上传重试";
  }
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

function AnswerSourcePanel() {
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

const KNOWLEDGE_RELATION_LABEL: Record<string, string> = {
  direct_assessment: "直接考查",
  rubric_basis: "评分点依据",
  answer_basis: "答案依据",
  context: "题干背景",
  prerequisite: "前置知识",
  distractor: "干扰项知识",
  misconception: "常见误区",
};

const ABILITY_MODE_LABEL: Record<string, string> = {
  recognition: "识别型作答",
  recall: "回忆型作答",
  structured_response: "结构化表达",
  source_analysis: "史料分析",
  argumentation: "论证评价",
};

function sourceDefaultRelation(source: LinkSuggestionSource) {
  if (source.requiredKnowledgeRelation) return source.requiredKnowledgeRelation;
  if (source.sourceType === "option") return "distractor";
  return "context";
}

function sourceDefaultMode(source: LinkSuggestionSource, type: K1QuestionType) {
  if (source.sourceType === "answer_slot") return "recall";
  if (source.sourceType === "rubric_point") return "structured_response";
  return type === "short_answer" ? "structured_response" : "recognition";
}

function emptyLinkSources(editor: LinkSuggestionInput): ConfirmedSourceLinksInput[] {
  return editor.sources.map((source) => ({
    sourceType: source.sourceType,
    sourcePublicId: source.sourcePublicId,
    knowledgeLinks: [],
    abilityLinks: [],
  }));
}

function KnowledgeLinkPanel() {
  const [catalog, setCatalog] = useState<Awaited<ReturnType<typeof loadLinkReviewCatalog>> | null>(null);
  const [items, setItems] = useState<LinkReviewInboxItem[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [mapId, setMapId] = useState("");
  const [editor, setEditor] = useState<LinkSuggestionInput | null>(null);
  const [links, setLinks] = useState<ConfirmedSourceLinksInput[]>([]);
  const [activeSuggestion, setActiveSuggestion] = useState<LinkSuggestionDraftView | null>(null);
  const [knowledgePicks, setKnowledgePicks] = useState<Record<string, string>>({});
  const [relationPicks, setRelationPicks] = useState<Record<string, string>>({});
  const [abilityPicks, setAbilityPicks] = useState<Record<string, string>>({});
  const [modePicks, setModePicks] = useState<Record<string, string>>({});
  const [strengthPicks, setStrengthPicks] = useState<Record<string, number>>({});
  const [note, setNote] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const selected = items.find((item) => item.questionVersionPublicId === selectedId) ?? null;

  const refresh = async () => {
    const [nextCatalog, nextItems] = await Promise.all([
      loadLinkReviewCatalog(),
      loadLinkReviewInbox(100),
    ]);
    setCatalog(nextCatalog);
    setItems(nextItems);
    setSelectedId((current) => (
      nextItems.some((item) => item.questionVersionPublicId === current)
        ? current
        : nextItems[0]?.questionVersionPublicId ?? ""
    ));
    setMapId((current) => (
      nextCatalog.maps.some((map) => map.publicId === current)
        ? current
        : nextCatalog.maps[0]?.publicId ?? ""
    ));
  };

  useEffect(() => {
    refresh().catch((reason) => setError(String(reason)));
    const handleAnswerConfirmed = () => {
      refresh().catch((reason) => setError(String(reason)));
    };
    window.addEventListener("k1-answer-confirmed", handleAnswerConfirmed);
    return () => window.removeEventListener("k1-answer-confirmed", handleAnswerConfirmed);
  }, []);

  const applySuggestion = (
    nextEditor: LinkSuggestionInput,
    draft: LinkSuggestionDraftView | null,
  ) => {
    const nextLinks = emptyLinkSources(nextEditor);
    if (draft?.knowledgeMapPublicId === nextEditor.knowledgeMapPublicId
      && draft.questionVersionPublicId === nextEditor.questionVersionPublicId) {
      draft.suggestion.sourceSuggestions.forEach((suggestion) => {
        const target = nextLinks.find((source) => (
          source.sourceType === suggestion.sourceType
          && source.sourcePublicId === suggestion.sourcePublicId
        ));
        if (!target) return;
        target.knowledgeLinks = suggestion.knowledgeLinks.map((link) => ({
          knowledgeNodePublicId: link.knowledgeNodePublicId,
          relationType: link.relationType,
        }));
        target.abilityLinks = suggestion.abilityLinks.map((link) => ({
          abilityDimensionPublicId: link.abilityDimensionPublicId,
          evidenceStrength: link.evidenceStrength,
          responseMode: link.responseMode,
        }));
      });
      setActiveSuggestion(draft);
    } else {
      setActiveSuggestion(null);
    }
    setLinks(nextLinks);
    setKnowledgePicks(Object.fromEntries(
      nextEditor.sources.map((source) => [
        source.sourcePublicId,
        nextEditor.knowledgeCandidates[0]?.publicId ?? "",
      ]),
    ));
    setRelationPicks(Object.fromEntries(
      nextEditor.sources.map((source) => [
        source.sourcePublicId,
        sourceDefaultRelation(source),
      ]),
    ));
    setAbilityPicks(Object.fromEntries(
      nextEditor.sources.map((source) => [
        source.sourcePublicId,
        nextEditor.abilityCandidates[0]?.publicId ?? "",
      ]),
    ));
    setModePicks(Object.fromEntries(
      nextEditor.sources.map((source) => [
        source.sourcePublicId,
        sourceDefaultMode(source, nextEditor.questionType),
      ]),
    ));
    setStrengthPicks(Object.fromEntries(
      nextEditor.sources.map((source) => [
        source.sourcePublicId,
        source.sourceType === "rubric_point" ? 0.7 : source.sourceType === "answer_slot" ? 0.6 : 0.4,
      ]),
    ));
  };

  useEffect(() => {
    if (!selectedId || !mapId) {
      setEditor(null);
      setLinks([]);
      return;
    }
    setWorking(true);
    setError("");
    loadLinkReviewEditor(selectedId, mapId)
      .then((nextEditor) => {
        setEditor(nextEditor);
        const draft = selected?.latestSuggestion?.knowledgeMapPublicId === mapId
          ? selected.latestSuggestion
          : null;
        applySuggestion(nextEditor, draft);
      })
      .catch((reason) => {
        setEditor(null);
        setLinks([]);
        setError(String(reason));
      })
      .finally(() => setWorking(false));
    // selected carries the latest draft for the selected id.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId, mapId, selected?.latestSuggestion?.publicId]);

  const updateSource = (
    sourcePublicId: string,
    update: (source: ConfirmedSourceLinksInput) => ConfirmedSourceLinksInput,
  ) => {
    setLinks((current) => current.map((source) => (
      source.sourcePublicId === sourcePublicId ? update(source) : source
    )));
  };

  const addKnowledge = (source: LinkSuggestionSource) => {
    const knowledgeNodePublicId = knowledgePicks[source.sourcePublicId];
    const relationType = relationPicks[source.sourcePublicId] ?? sourceDefaultRelation(source);
    if (!knowledgeNodePublicId) return;
    updateSource(source.sourcePublicId, (current) => {
      if (current.knowledgeLinks.some((link) => (
        link.knowledgeNodePublicId === knowledgeNodePublicId
        && link.relationType === relationType
      ))) return current;
      return {
        ...current,
        knowledgeLinks: [...current.knowledgeLinks, { knowledgeNodePublicId, relationType }],
      };
    });
  };

  const addAbility = (source: LinkSuggestionSource) => {
    const abilityDimensionPublicId = abilityPicks[source.sourcePublicId];
    const responseMode = modePicks[source.sourcePublicId]
      ?? sourceDefaultMode(source, editor?.questionType ?? "single");
    const evidenceStrength = Number(strengthPicks[source.sourcePublicId] ?? 0);
    if (!abilityDimensionPublicId || evidenceStrength <= 0 || evidenceStrength > 1) return;
    updateSource(source.sourcePublicId, (current) => {
      if (current.abilityLinks.some((link) => (
        link.abilityDimensionPublicId === abilityDimensionPublicId
        && link.responseMode === responseMode
      ))) return current;
      return {
        ...current,
        abilityLinks: [
          ...current.abilityLinks,
          { abilityDimensionPublicId, responseMode, evidenceStrength },
        ],
      };
    });
  };

  const generate = async () => {
    if (!selected || !mapId) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const result = await suggestKnowledgeLinks(
        selected.questionVersionPublicId,
        mapId,
        newLinkRequestKey("suggest"),
      );
      if (result.status === "failed" || !result.suggestion) {
        setError(result.failure?.safe_message ?? "AI 建议失败，可直接手工关联。");
        return;
      }
      const nextEditor = await loadLinkReviewEditor(selected.questionVersionPublicId, mapId);
      setEditor(nextEditor);
      applySuggestion(nextEditor, result.suggestion);
      setNotice(
        result.suggestion.suggestion.state === "ready"
          ? "AI 已按现有目录给出完整草稿，请逐项核对后确认。"
          : "AI 已给出草稿；存在不确定项，请补齐后确认。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const complete = Boolean(editor) && editor!.sources.every((source) => {
    if (!source.requiredForL3) return true;
    const current = links.find((item) => item.sourcePublicId === source.sourcePublicId);
    return Boolean(
      current?.knowledgeLinks.some((link) => (
        link.relationType === source.requiredKnowledgeRelation
      ))
      && current?.abilityLinks.some((link) => link.evidenceStrength > 0),
    );
  });

  const confirm = async () => {
    if (!selected || !editor || !complete) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const result = await confirmKnowledgeLinks({
        requestKey: newLinkRequestKey("confirm"),
        questionVersionPublicId: selected.questionVersionPublicId,
        expectedQuestionContentHash: selected.questionContentHash,
        knowledgeMapPublicId: mapId,
        suggestionDraftPublicId: activeSuggestion?.publicId ?? null,
        expectedSuggestionContentHash: activeSuggestion?.contentHash ?? null,
        sources: links,
        reviewedBy: "local_teacher",
        note: note.trim() || null,
      });
      setNotice(
        `已确认 ${result.knowledgeLinkCount} 条知识链接和 ${result.abilityLinkCount} 条能力链接，题目已晋级 L3。`,
      );
      setNote("");
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const relationOptions = (source: LinkSuggestionSource) => {
    if (source.sourceType === "question") return ["direct_assessment", "context", "prerequisite"];
    if (source.sourceType === "option") return ["answer_basis", "distractor", "misconception", "context"];
    if (source.sourceType === "answer_slot") return ["direct_assessment", "answer_basis"];
    return ["rubric_basis"];
  };

  return (
    <section className="dashboard-panel knowledge-link-panel">
      <div className="dashboard-panel-head">
        <div>
          <b>关联知识点与能力</b>
          <span>答案和评分点确认后，AI 先整理草稿；老师逐项确认才进入学习图谱</span>
        </div>
        <span className={items.length ? "tag warn" : "tag ok"}>
          {items.length} 道 L2 待关联
        </span>
      </div>
      {error && <div className="error">{error}</div>}
      {notice && <div className="ok-banner">{notice}</div>}
      {!items.length ? (
        <div className="muted source-empty">暂无待关联题目。补齐答案和评分点后，L2 题目会自动出现在这里。</div>
      ) : !catalog?.maps.length ? (
        <div className="empty-state">还没有已确认教材知识地图，暂时不能晋级 L3。</div>
      ) : (
        <>
          <div className="knowledge-link-toolbar">
            <label className="field">
              <span className="fl">待关联题目</span>
              <select value={selectedId} onChange={(event) => setSelectedId(event.target.value)}>
                {items.map((item) => (
                  <option key={item.questionVersionPublicId} value={item.questionVersionPublicId}>
                    {TYPE_LABEL[item.questionType]} · {item.stem}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="fl">教材知识地图</span>
              <select value={mapId} onChange={(event) => setMapId(event.target.value)}>
                {catalog.maps.map((map) => (
                  <option key={map.publicId} value={map.publicId}>
                    {map.title} · 第 {map.revision} 版
                  </option>
                ))}
              </select>
            </label>
            <button className="primary" disabled={working || !selected || !mapId} onClick={generate}>
              {working ? "正在整理…" : "AI 整理链接草稿"}
            </button>
          </div>
          {editor && (
            <>
              <div className="knowledge-link-summary">
                <div>
                  <b>{TYPE_LABEL[editor.questionType]} · {editor.stem}</b>
                  <span>
                    {editor.textbookTitle} · {editor.knowledgeCandidates.length} 个知识点 ·
                    {" "}{editor.abilityCandidates.length} 个能力维度
                  </span>
                </div>
                <span className={activeSuggestion?.suggestion.state === "ready" ? "tag ok" : "tag"}>
                  {activeSuggestion
                    ? `AI 草稿 ${Math.round(activeSuggestion.suggestion.confidence * 100)}%`
                    : "老师手工复核"}
                </span>
              </div>
              <div className="knowledge-link-source-list">
                {editor.sources.map((source) => {
                  const current = links.find((item) => item.sourcePublicId === source.sourcePublicId);
                  return (
                    <article key={source.sourcePublicId} className="knowledge-link-source">
                      <div className="knowledge-link-source-head">
                        <div>
                          <b>{source.label}</b>
                          <span>{source.detail}</span>
                        </div>
                        <span className={source.requiredForL3 ? "tag warn" : "tag"}>
                          {source.requiredForL3 ? "L3 必需" : "可选补充"}
                        </span>
                      </div>
                      <div className="knowledge-link-chips">
                        {current?.knowledgeLinks.map((link, index) => {
                          const node = editor.knowledgeCandidates.find(
                            (candidate) => candidate.publicId === link.knowledgeNodePublicId,
                          );
                          return (
                            <span className="link-chip" key={`${link.knowledgeNodePublicId}-${link.relationType}`}>
                              知识 · {node?.title ?? link.knowledgeNodePublicId} ·
                              {" "}{KNOWLEDGE_RELATION_LABEL[link.relationType] ?? link.relationType}
                              <button
                                aria-label={`删除知识链接 ${index + 1}`}
                                onClick={() => updateSource(source.sourcePublicId, (value) => ({
                                  ...value,
                                  knowledgeLinks: value.knowledgeLinks.filter((_, itemIndex) => itemIndex !== index),
                                }))}
                              >
                                ×
                              </button>
                            </span>
                          );
                        })}
                        {current?.abilityLinks.map((link, index) => {
                          const ability = editor.abilityCandidates.find(
                            (candidate) => candidate.publicId === link.abilityDimensionPublicId,
                          );
                          return (
                            <span className="link-chip ability" key={`${link.abilityDimensionPublicId}-${link.responseMode}`}>
                              能力 · {ability?.title ?? link.abilityDimensionPublicId} ·
                              {" "}{ABILITY_MODE_LABEL[link.responseMode] ?? link.responseMode} ·
                              {" "}{Math.round(link.evidenceStrength * 100)}%
                              <button
                                aria-label={`删除能力链接 ${index + 1}`}
                                onClick={() => updateSource(source.sourcePublicId, (value) => ({
                                  ...value,
                                  abilityLinks: value.abilityLinks.filter((_, itemIndex) => itemIndex !== index),
                                }))}
                              >
                                ×
                              </button>
                            </span>
                          );
                        })}
                      </div>
                      <div className="knowledge-link-add-row">
                        <select
                          aria-label={`${source.label} 知识点`}
                          value={knowledgePicks[source.sourcePublicId] ?? ""}
                          onChange={(event) => setKnowledgePicks((currentPicks) => ({
                            ...currentPicks,
                            [source.sourcePublicId]: event.target.value,
                          }))}
                        >
                          {editor.knowledgeCandidates.map((candidate) => (
                            <option key={candidate.publicId} value={candidate.publicId}>
                              {candidate.curriculumTitle ? `${candidate.curriculumTitle} · ` : ""}
                              {candidate.title}
                            </option>
                          ))}
                        </select>
                        <select
                          aria-label={`${source.label} 知识关系`}
                          value={relationPicks[source.sourcePublicId] ?? sourceDefaultRelation(source)}
                          onChange={(event) => setRelationPicks((currentPicks) => ({
                            ...currentPicks,
                            [source.sourcePublicId]: event.target.value,
                          }))}
                        >
                          {relationOptions(source).map((relation) => (
                            <option key={relation} value={relation}>
                              {KNOWLEDGE_RELATION_LABEL[relation]}
                            </option>
                          ))}
                        </select>
                        <button onClick={() => addKnowledge(source)}>添加知识点</button>
                      </div>
                      <div className="knowledge-link-add-row ability-row">
                        <select
                          aria-label={`${source.label} 能力维度`}
                          value={abilityPicks[source.sourcePublicId] ?? ""}
                          onChange={(event) => setAbilityPicks((currentPicks) => ({
                            ...currentPicks,
                            [source.sourcePublicId]: event.target.value,
                          }))}
                        >
                          {editor.abilityCandidates.map((candidate) => (
                            <option key={candidate.publicId} value={candidate.publicId}>
                              {candidate.title}
                            </option>
                          ))}
                        </select>
                        <select
                          aria-label={`${source.label} 作答方式`}
                          value={modePicks[source.sourcePublicId] ?? sourceDefaultMode(source, editor.questionType)}
                          onChange={(event) => setModePicks((currentPicks) => ({
                            ...currentPicks,
                            [source.sourcePublicId]: event.target.value,
                          }))}
                        >
                          {Object.entries(ABILITY_MODE_LABEL).map(([value, label]) => (
                            <option key={value} value={value}>{label}</option>
                          ))}
                        </select>
                        <input
                          aria-label={`${source.label} 证据强度`}
                          type="number"
                          min={0.1}
                          max={1}
                          step={0.1}
                          value={strengthPicks[source.sourcePublicId] ?? 0.4}
                          onChange={(event) => setStrengthPicks((currentPicks) => ({
                            ...currentPicks,
                            [source.sourcePublicId]: Number(event.target.value),
                          }))}
                        />
                        <button onClick={() => addAbility(source)}>添加能力</button>
                      </div>
                    </article>
                  );
                })}
              </div>
              <div className="knowledge-link-confirm">
                <label className="field">
                  <span className="fl">老师备注（可选）</span>
                  <input value={note} maxLength={300} onChange={(event) => setNote(event.target.value)} />
                </label>
                <div>
                  <span>
                    {complete
                      ? "必需来源已覆盖；确认只晋级题库版本，不会改写历史作业。"
                      : "请为每个必需来源补齐直接知识链接和能力链接。"}
                  </span>
                  <button className="primary" disabled={working || !complete} onClick={confirm}>
                    {working ? "正在确认…" : "确认链接并晋级 L3"}
                  </button>
                </div>
              </div>
            </>
          )}
        </>
      )}
    </section>
  );
}

function SourceImportPanel() {
  const [sourceType, setSourceType] = useState<"blank_paper" | "source_document">("source_document");
  const [path, setPath] = useState("");
  const [items, setItems] = useState<SourceInboxItem[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [questionType, setQuestionType] = useState<K1QuestionType>("single");
  const [stem, setStem] = useState("");
  const [materialText, setMaterialText] = useState("");
  const [maxScore, setMaxScore] = useState(1);
  const [options, setOptions] = useState<SourceDraft["options"]>([]);
  const [note, setNote] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const pendingDrafts = useMemo(
    () => items.flatMap((item) => (
      item.latestExtraction?.drafts
        .filter((draft) => !draft.reviewAction)
        .map((draft) => ({ draft, item })) ?? []
    )),
    [items],
  );
  const selectedEntry = pendingDrafts.find(({ draft }) => draft.publicId === selectedId) ?? null;
  const selected = selectedEntry?.draft ?? null;

  const populate = (draft: SourceDraft | null) => {
    setSelectedId(draft?.publicId ?? "");
    setQuestionType(draft?.questionType ?? "single");
    setStem(draft?.stem ?? "");
    setMaterialText(draft?.materialText ?? "");
    setMaxScore(draft?.maxScore ?? 1);
    setOptions(draft?.options.map((option) => ({ ...option })) ?? []);
    setNote("");
  };

  const refresh = async (preferredId?: string) => {
    const nextItems = await loadSourceInbox(50);
    setItems(nextItems);
    const nextDrafts = nextItems.flatMap((item) => (
      item.latestExtraction?.drafts.filter((draft) => !draft.reviewAction) ?? []
    ));
    populate(
      nextDrafts.find((draft) => draft.publicId === preferredId)
      ?? nextDrafts[0]
      ?? null,
    );
  };

  useEffect(() => {
    refresh().catch((reason) => setError(String(reason)));
  }, []);

  const chooseFile = async () => {
    const selectedPath = await open({
      multiple: false,
      directory: false,
      filters: [{
        name: "题目文件",
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
    if (!path) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const result = await importAndAnalyzeSource(
        path,
        sourceType,
        newSourceRequestKey("import"),
      );
      await refresh(result.extraction?.drafts[0]?.publicId);
      if (result.status === "failed") {
        setError(result.failure?.safeMessage ?? "题目识别失败，来源已经保留，可稍后重试。");
      } else if (result.extraction?.extractionState === "blocked") {
        setError("检测到学生信息、作答或教师批注，已阻止题面进入题库。请改用空白卷或干净电子文件。");
      } else {
        const count = result.extraction?.drafts.length ?? 0;
        setNotice(`已识别 ${count} 道题。无异常内容可一次确认，模糊项请在下方修正。`);
        setPath("");
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const corrected = (): AcceptSourceDraftInput["corrected"] => ({
    questionType,
    stem: stem.trim(),
    materialText: materialText.trim() || null,
    maxScore,
    options: questionType === "single" || questionType === "multiple"
      ? options.map((option, index) => ({
        label: option.label.trim(),
        content: option.content.trim(),
        orderIndex: index + 1,
      }))
      : [],
  });

  const acceptOne = async () => {
    if (!selected) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const review = await acceptSourceDraft({
        requestKey: newSourceRequestKey("accept"),
        draftPublicId: selected.publicId,
        expectedContentHash: selected.contentHash,
        corrected: corrected(),
        note: note.trim() || null,
      });
      await refresh();
      setNotice(
        review.resultKind === "exact_reused"
          ? "已复用个人题库中的完全相同题目，没有创建重复题。"
          : "已保存为 L0 待完善题目。补充答案后才会成为可练习或可批改题。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const discardOne = async () => {
    if (!selected) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      await discardSourceDraft(
        selected.publicId,
        selected.contentHash,
        note.trim() || null,
        newSourceRequestKey("discard"),
      );
      await refresh();
      setNotice("已忽略这道题；原始来源和处理记录仍可追溯。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const acceptReadyBatch = async (item: SourceInboxItem) => {
    const drafts = item.latestExtraction?.drafts.filter((draft) => !draft.reviewAction) ?? [];
    if (!drafts.length) return;
    setWorking(true);
    setError("");
    setNotice("");
    let confirmed = 0;
    try {
      for (const draft of drafts) {
        await acceptSourceDraft({
          requestKey: newSourceRequestKey("accept"),
          draftPublicId: draft.publicId,
          expectedContentHash: draft.contentHash,
          corrected: {
            questionType: draft.questionType,
            stem: draft.stem,
            materialText: draft.materialText,
            maxScore: draft.maxScore,
            options: draft.options.map((option, index) => ({
              label: option.label,
              content: option.content,
              orderIndex: index + 1,
            })),
          },
          note: "老师批量确认本次高置信度题面",
        });
        confirmed += 1;
      }
      await refresh();
      setNotice(`已确认 ${confirmed} 道无异常题面；完全重复题已自动复用，其余保存为 L0 待完善题目。`);
    } catch (reason) {
      await refresh();
      setError(`已确认 ${confirmed} 道，后续项目停止：${String(reason)}`);
    } finally {
      setWorking(false);
    }
  };

  return (
    <>
      {error && <div className="error">{error}</div>}
      {notice && <div className="ok-banner">{notice}</div>}
      <section className="dashboard-panel source-import-hero">
        <div className="dashboard-panel-head">
          <div>
            <b>把现成题目直接放进我的题库</b>
            <span>支持 JPG、PDF、Word、Excel 和 TXT；系统自动拆题、查重，只让你处理模糊项</span>
          </div>
          <span className="tag">私有草稿，不会共享</span>
        </div>
        <div className="source-import-flow">
          <div className="source-type-choice">
            <button
              className={sourceType === "source_document" ? "active" : ""}
              onClick={() => setSourceType("source_document")}
            >
              <b>电子题目文件</b>
              <span>Word、Excel、PDF、TXT</span>
            </button>
            <button
              className={sourceType === "blank_paper" ? "active" : ""}
              onClick={() => setSourceType("blank_paper")}
            >
              <b>空白试卷照片</b>
              <span>没有学生作答和批注的 JPG/PDF</span>
            </button>
          </div>
          <div className="source-file-row">
            <button onClick={chooseFile} disabled={working}>
              {path ? "重新选择" : "选择题目文件"}
            </button>
            <div>
              <b>{path ? sourceFileLabel(path) : "还没有选择文件"}</b>
              <span>含姓名、手写答案、分数或教师批注的学生卷会被阻止进入题库</span>
            </div>
            <button className="primary" disabled={working || !path} onClick={analyze}>
              {working ? "正在识别…" : "识别并整理题目"}
            </button>
          </div>
        </div>
      </section>

      <section className="dashboard-panel source-inbox-summary">
        <div className="dashboard-panel-head">
          <div>
            <b>最近导入</b>
            <span>识别结果先停在这里；确认题面后仍需补答案，才可用于练习或批改</span>
          </div>
          <span className={pendingDrafts.length ? "tag warn" : "tag ok"}>
            {pendingDrafts.length} 道待确认
          </span>
        </div>
        {!items.length ? (
          <div className="muted source-empty">选择一个干净题目文件，就可以开始自动整理。</div>
        ) : (
          <div className="source-document-list">
            {items.map((item) => {
              const extraction = item.latestExtraction;
              const canBatch = extraction?.extractionState === "ready"
                && extraction.issueCodes.length === 0
                && item.pendingDrafts > 0;
              return (
                <div key={item.document.publicId} className="source-document-row">
                  <div>
                    <b>{sourceTypeLabel(item.document.sourceType)} · {item.document.sourceFormat.toUpperCase()}</b>
                    <span>
                      {item.document.pageCount} 页 ·
                      {extraction
                        ? ` 识别 ${extraction.drafts.length} 道，待确认 ${item.pendingDrafts} 道`
                        : item.latestAiStatus === "failed"
                          ? ` ${sourceFailureMessage(item)}`
                          : item.latestAiStatus === "processing"
                            ? " 正在识别"
                            : " 等待识别"}
                    </span>
                  </div>
                  {item.latestAiStatus === "failed" ? (
                    <span className="tag warn">识别失败 · 可重试</span>
                  ) : extraction?.extractionState === "blocked" ? (
                    <span className="tag warn">隐私或内容检查已阻断</span>
                  ) : canBatch ? (
                    <button className="primary" disabled={working} onClick={() => acceptReadyBatch(item)}>
                      确认本批 {item.pendingDrafts} 道无异常题面
                    </button>
                  ) : (
                    <span className={item.pendingDrafts ? "tag warn" : "tag ok"}>
                      {item.pendingDrafts ? "需要逐题核对" : "已处理"}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>

      {pendingDrafts.length > 0 && (
        <div className="source-review-layout">
          <aside className="dashboard-panel source-review-list">
            {pendingDrafts.map(({ draft, item }) => (
              <button
                key={draft.publicId}
                className={draft.publicId === selectedId ? "active" : ""}
                onClick={() => populate(draft)}
              >
                <span>
                  {draft.questionNo ? `第 ${draft.questionNo} 题` : `第 ${draft.orderIndex} 题`}
                  {" · "}
                  {TYPE_LABEL[draft.questionType]}
                  {" · "}
                  {Math.round(draft.confidence * 100)}%
                </span>
                <b>{draft.stem}</b>
                <small>{sourceTypeLabel(item.document.sourceType)}</small>
              </button>
            ))}
          </aside>
          {selected && (
            <section className="dashboard-panel source-review-editor">
              <div className="dashboard-panel-head">
                <div>
                  <b>核对模糊项</b>
                  <span>这里只确认题面结构；系统不会替你编答案</span>
                </div>
                <span className="tag">确认后 L0</span>
              </div>
              <div className="candidate-editor-grid">
                <label className="field">
                  <span className="fl">题型</span>
                  <select value={questionType} onChange={(event) => setQuestionType(event.target.value as K1QuestionType)}>
                    {TYPE_ORDER.map((type) => <option key={type} value={type}>{TYPE_LABEL[type]}</option>)}
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
              {(questionType === "single" || questionType === "multiple") && (
                <div className="candidate-option-editor">
                  <span className="fl">选项</span>
                  {options.map((option, index) => (
                    <div key={`${index}-${option.label}`}>
                      <input
                        aria-label={`题面选项 ${index + 1} 标签`}
                        value={option.label}
                        onChange={(event) => setOptions((current) => current.map(
                          (value, currentIndex) => currentIndex === index
                            ? { ...value, label: event.target.value }
                            : value,
                        ))}
                      />
                      <input
                        aria-label={`题面选项 ${index + 1} 内容`}
                        value={option.content}
                        onChange={(event) => setOptions((current) => current.map(
                          (value, currentIndex) => currentIndex === index
                            ? { ...value, content: event.target.value }
                            : value,
                        ))}
                      />
                      <button
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
                    {
                      label: String.fromCharCode(65 + current.length),
                      content: "",
                      order_index: current.length + 1,
                    },
                  ])}>
                    增加选项
                  </button>
                </div>
              )}
              <label className="field">
                <span className="fl">备注（可选）</span>
                <input value={note} maxLength={300} onChange={(event) => setNote(event.target.value)} />
              </label>
              <div className="candidate-review-actions">
                <div>
                  <b>确认后只保存题面草稿</b>
                  <span>下一步可上传答案文件，或在题库中补答案和评分点。</span>
                </div>
                <button disabled={working} onClick={discardOne}>忽略这道题</button>
                <button
                  className="primary"
                  disabled={working || !stem.trim() || maxScore <= 0
                    || ((questionType === "single" || questionType === "multiple")
                      && (options.length < 2 || options.some((option) => !option.label.trim() || !option.content.trim())))}
                  onClick={acceptOne}
                >
                  {working ? "正在保存…" : "确认题面"}
                </button>
              </div>
            </section>
          )}
        </div>
      )}
      <AnswerSourcePanel
        key={items.map((item) => (
          `${item.document.publicId}:${item.pendingDrafts}:${item.totalDrafts}`
        )).join("|")}
      />
      <KnowledgeLinkPanel />
    </>
  );
}

function QuestionSearchPanel({ options }: { options: BlueprintOptions | null }) {
  const [query, setQuery] = useState("");
  const [ownerScope, setOwnerScope] = useState<"all" | "personal" | "official">("all");
  const [questionType, setQuestionType] = useState<K1QuestionType | "">("");
  const [minimumQuality, setMinimumQuality] = useState<QuestionSearchInput["minimumQuality"]>("L0");
  const [duplicateOnly, setDuplicateOnly] = useState(false);
  const [mapPublicId, setMapPublicId] = useState("");
  const [curriculumPublicId, setCurriculumPublicId] = useState("");
  const [knowledgePublicId, setKnowledgePublicId] = useState("");
  const [result, setResult] = useState<QuestionSearchResponse | null>(null);
  const [working, setWorking] = useState(false);
  const [reviewing, setReviewing] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const curriculumOptions = useMemo(
    () => options?.curriculum_nodes.filter(
      (node) => node.knowledge_map_public_id === mapPublicId,
    ) ?? [],
    [options, mapPublicId],
  );
  const scopeCurriculumIds = useMemo(() => {
    if (!curriculumPublicId) return null;
    const values = new Set([curriculumPublicId]);
    let changed = true;
    while (changed) {
      changed = false;
      curriculumOptions.forEach((node) => {
        if (node.parent_public_id && values.has(node.parent_public_id) && !values.has(node.public_id)) {
          values.add(node.public_id);
          changed = true;
        }
      });
    }
    return values;
  }, [curriculumOptions, curriculumPublicId]);
  const knowledgeOptions = useMemo(
    () => options?.knowledge_nodes.filter((node) => (
      node.knowledge_map_public_id === mapPublicId
      && (!scopeCurriculumIds
        || Boolean(node.curriculum_node_public_id && scopeCurriculumIds.has(node.curriculum_node_public_id)))
    )) ?? [],
    [options, mapPublicId, scopeCurriculumIds],
  );

  const input = useMemo<QuestionSearchInput>(() => ({
    query,
    ownerScope,
    questionType: questionType || null,
    minimumQuality,
    state: "active",
    knowledgeMapPublicId: mapPublicId || null,
    curriculumNodePublicId: curriculumPublicId || null,
    knowledgeNodePublicId: knowledgePublicId || null,
    duplicateOnly,
    limit: 50,
    offset: 0,
  }), [
    query,
    ownerScope,
    questionType,
    minimumQuality,
    mapPublicId,
    curriculumPublicId,
    knowledgePublicId,
    duplicateOnly,
  ]);

  const runSearch = async () => {
    setWorking(true);
    setError("");
    setNotice("");
    try {
      setResult(await searchQuestions(input));
    } catch (reason) {
      setResult(null);
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  useEffect(() => {
    if (options) void runSearch();
    // 首次进入自动列出可访问题目；后续由老师点击搜索，避免每次输入都调用后端。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options]);

  const decide = async (
    leftVersionPublicId: string,
    candidate: DuplicateCandidate,
    decision: "independent" | "same_family",
  ) => {
    const key = `${leftVersionPublicId}-${candidate.question_version_public_id}`;
    setReviewing(key);
    setError("");
    try {
      await reviewDuplicate({
        requestKey: newDuplicateRequestKey(),
        leftQuestionVersionPublicId: leftVersionPublicId,
        rightQuestionVersionPublicId: candidate.question_version_public_id,
        decision,
        note: null,
      });
      setNotice(
        decision === "same_family"
          ? "已标记为同题变式；两道题仍保持独立，不会改写历史作业。"
          : "已确认是不同题；系统保留本次老师结论。",
      );
      setResult(await searchQuestions(input));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setReviewing("");
    }
  };

  return (
    <>
      {error && <div className="error">{error}</div>}
      {notice && <div className="ok-banner">{notice}</div>}
      <section className="dashboard-panel question-search-panel">
        <div className="dashboard-panel-head">
          <div><b>找题与查重</b><span>输入关键词，也可以按题型、来源和教材范围筛选</span></div>
          <span className="tag">当前版本</span>
        </div>
        <div className="question-search-main">
          <label className="field question-search-keyword">
            <span className="fl">关键词</span>
            <input
              value={query}
              maxLength={100}
              placeholder="题干、材料或选项，如：洋务运动"
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void runSearch();
              }}
            />
          </label>
          <label className="field">
            <span className="fl">题型</span>
            <select value={questionType} onChange={(event) => setQuestionType(event.target.value as K1QuestionType | "")}>
              <option value="">全部题型</option>
              {TYPE_ORDER.map((type) => <option key={type} value={type}>{TYPE_LABEL[type]}</option>)}
            </select>
          </label>
          <label className="field">
            <span className="fl">来源</span>
            <select value={ownerScope} onChange={(event) => setOwnerScope(event.target.value as typeof ownerScope)}>
              <option value="all">我的 + 官方</option>
              <option value="personal">只看我的</option>
              <option value="official">只看官方</option>
            </select>
          </label>
          <label className="field">
            <span className="fl">最低可用等级</span>
            <select value={minimumQuality} onChange={(event) => setMinimumQuality(event.target.value as typeof minimumQuality)}>
              <option value="L0">全部已结构化题</option>
              <option value="L1">可练习</option>
              <option value="L2">可自动批改</option>
              <option value="L3">可用于图谱</option>
              <option value="L4">可共享</option>
            </select>
          </label>
          <button className="primary" disabled={working} onClick={runSearch}>
            {working ? "正在查找…" : "查找题目"}
          </button>
        </div>
        <details className="question-search-advanced">
          <summary>按教材、章节或知识点进一步筛选</summary>
          <div>
            <label className="field">
              <span className="fl">教材知识地图</span>
              <select value={mapPublicId} onChange={(event) => {
                setMapPublicId(event.target.value);
                setCurriculumPublicId("");
                setKnowledgePublicId("");
              }}>
                <option value="">不限教材</option>
                {options?.knowledge_maps.map((map) => (
                  <option key={map.public_id} value={map.public_id}>{map.title} · 第 {map.revision} 版</option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="fl">章节范围</span>
              <select disabled={!mapPublicId} value={curriculumPublicId} onChange={(event) => {
                setCurriculumPublicId(event.target.value);
                setKnowledgePublicId("");
              }}>
                <option value="">不限章节</option>
                {curriculumOptions.map((node) => (
                  <option key={node.public_id} value={node.public_id}>{node.title}</option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="fl">知识点</span>
              <select disabled={!mapPublicId} value={knowledgePublicId} onChange={(event) => setKnowledgePublicId(event.target.value)}>
                <option value="">不限知识点</option>
                {knowledgeOptions.map((node) => (
                  <option key={node.public_id} value={node.public_id}>{node.title}</option>
                ))}
              </select>
            </label>
            <label className="question-search-check">
              <input type="checkbox" checked={duplicateOnly} onChange={(event) => setDuplicateOnly(event.target.checked)} />
              只看有重复提示的题
            </label>
          </div>
        </details>
        {result && (
          <div className="question-search-summary">
            <b>找到 {result.total} 道题</b>
            <span>{result.boundary_note}</span>
          </div>
        )}
      </section>

      <section className="question-search-results">
        {result?.items.map((item) => (
          <article className="question-search-card" key={item.question_version_public_id}>
            <div className="question-search-card-head">
              <div>
                <span className="tag">{TYPE_LABEL[item.question_type]}</span>
                <span className="tag">{item.owner_label}</span>
                <span className="tag">{item.quality_level}</span>
              </div>
              <strong>{item.max_score} 分</strong>
            </div>
            <h3>{item.stem}</h3>
            {item.material_text && <p>{item.material_text}</p>}
            {item.options.length > 0 && (
              <div className="question-search-options">
                {item.options.map((option) => <span key={option.label}>{option.label}. {option.content}</span>)}
              </div>
            )}
            <div className="blueprint-evidence-tags">
              {item.knowledge_nodes.map((node) => (
                <span key={`${node.public_id}-${node.relation_type}`}>知识 · {node.title}</span>
              ))}
              {item.ability_dimensions.map((ability) => (
                <span key={`${ability.public_id}-${ability.response_mode}`}>能力 · {ability.title}</span>
              ))}
              <span>已用于 {item.assessment_usage_count} 次作业</span>
            </div>
            {item.duplicate_candidates.length > 0 && (
              <div className="duplicate-candidates">
                <b>重复提示</b>
                {item.duplicate_candidates.map((candidate) => {
                  const key = `${item.question_version_public_id}-${candidate.question_version_public_id}`;
                  return (
                    <div className="duplicate-candidate-row" key={candidate.question_version_public_id}>
                      <div>
                        <span className={candidate.decision ? "tag ok" : "tag warn"}>
                          {duplicateLabel(candidate)} · {Math.round(candidate.similarity * 100)}%
                        </span>
                        <p>{candidate.stem}</p>
                      </div>
                      <div>
                        <button
                          disabled={reviewing === key}
                          onClick={() => decide(item.question_version_public_id, candidate, "independent")}
                        >
                          确认是不同题
                        </button>
                        <button
                          className="primary"
                          disabled={reviewing === key}
                          onClick={() => decide(item.question_version_public_id, candidate, "same_family")}
                        >
                          标记为同题变式（仅归类，不合并）
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </article>
        ))}
        {result && result.items.length === 0 && (
          <div className="dashboard-panel muted">没有符合条件的题，试试放宽筛选条件。</div>
        )}
      </section>
    </>
  );
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

function CandidateReviewPanel() {
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

const ASSESSMENT_CONTEXT_LABEL: Record<string, string> = {
  homework: "日常作业",
  quiz: "随堂测验",
  exam: "正式考试",
  practice: "练习",
  correction: "订正",
};

function percentage(value: number | null) {
  return value == null ? "暂无" : `${Math.round(value * 100)}%`;
}

function changeLabels(row: QuestionVersionImpactPreview["rows"][number]) {
  const labels: string[] = [];
  if (row.answerChanged) labels.push("答案");
  if (row.rubricChanged) labels.push("评分点");
  if (row.linkChanged) labels.push("知识链接");
  return labels.join("、") || "无";
}

function readableAnswerJson(raw: string) {
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    const labels = value.correct_labels ?? value.correctLabels;
    if (Array.isArray(labels)) return labels.join("、");
    const answer = value.answer ?? value.correct_answer ?? value.correctAnswer;
    if (typeof answer === "string") return answer;
    return JSON.stringify(value, null, 2);
  } catch {
    return raw;
  }
}

type ImpactComponentEditor = {
  teacherScore: number;
  evidenceText: string;
  teacherNote: string;
};

function ImpactReviewCaseCard({
  reviewCase,
  disabled,
  onResolved,
  onPublished,
  onOpenExam,
}: {
  reviewCase: QuestionImpactReviewCase;
  disabled: boolean;
  onResolved: (
    reviewCase: QuestionImpactReviewCase,
    teacherScore: number | null,
    components: Array<{
      sourcePublicId: string;
      teacherScore: number;
      evidenceText: string | null;
      teacherNote: string | null;
    }>,
    note: string,
  ) => Promise<void>;
  onPublished: (reviewCase: QuestionImpactReviewCase) => Promise<void>;
  onOpenExam: () => void;
}) {
  const [teacherScore, setTeacherScore] = useState(
    reviewCase.sourceTeacherScore ?? reviewCase.maxScore,
  );
  const [note, setNote] = useState("");
  const [components, setComponents] = useState<Record<string, ImpactComponentEditor>>(
    Object.fromEntries(reviewCase.targetComponents.map((component) => [
      component.sourcePublicId,
      { teacherScore: 0, evidenceText: "", teacherNote: "" },
    ])),
  );
  const isObjective = reviewCase.targetComponents.length === 0;
  const canResolve = note.trim().length > 0 && (
    isObjective
      ? Number.isFinite(teacherScore)
        && teacherScore >= 0
        && teacherScore <= reviewCase.maxScore
      : reviewCase.targetComponents.every((component) => {
        const editor = components[component.sourcePublicId];
        return editor
          && Number.isFinite(editor.teacherScore)
          && editor.teacherScore >= 0
          && editor.teacherScore <= component.maxScore
          && (editor.teacherScore === 0 || editor.evidenceText.trim().length > 0);
      })
  );
  const submitResolution = () => onResolved(
    reviewCase,
    isObjective ? teacherScore : null,
    reviewCase.targetComponents.map((component) => {
      const editor = components[component.sourcePublicId];
      return {
        sourcePublicId: component.sourcePublicId,
        teacherScore: editor.teacherScore,
        evidenceText: editor.evidenceText.trim() || null,
        teacherNote: editor.teacherNote.trim() || null,
      };
    }),
    note.trim(),
  );

  return (
    <article className={`impact-review-card state-${reviewCase.state}`}>
      <div className="impact-case-head">
        <div>
          <b>{reviewCase.studentNo} · {reviewCase.studentName}</b>
          <span>
            {reviewCase.className} · {reviewCase.assessmentTitle} · 第 {reviewCase.questionNo} 题
          </span>
        </div>
        <span className={reviewCase.caseKind === "published_review" ? "tag warning" : "tag subtle"}>
          {reviewCase.caseKind === "published_review" ? "已发布复核" : "未发布重评"}
        </span>
      </div>
      <div className="impact-case-meta">
        <span>
          旧评分：
          {reviewCase.sourceTeacherScore == null
            ? "暂无"
            : `${reviewCase.sourceTeacherScore} 分（第 ${reviewCase.sourceGradeDecisionRevision} 版）`}
        </span>
        <span>目标答案第 {reviewCase.targetAnswerKeyRevision} 版</span>
        <span>目标评分点第 {reviewCase.targetRubricRevision} 版</span>
        <span>目标知识链接第 {reviewCase.targetLinkSetRevision} 版</span>
      </div>
      <div className="impact-evidence-grid">
        <div>
          <b>题目与目标答案</b>
          <p>{reviewCase.questionStem}</p>
          <pre>{readableAnswerJson(reviewCase.targetAnswerJson)}</pre>
        </div>
        <div>
          <b>学生原作答</b>
          {reviewCase.cropPath && (
            <img
              className="impact-answer-crop"
              src={convertFileSrc(reviewCase.cropPath)}
              alt={`${reviewCase.studentName} 第 ${reviewCase.questionNo} 题原作答`}
            />
          )}
          <pre>{reviewCase.studentResponseText || "当前没有可用的识别文本，请回批改台核对原图。"}</pre>
        </div>
      </div>
      {reviewCase.state === "open" ? (
        <div className="impact-resolution-editor">
          {isObjective ? (
            <label className="field compact">
              <span className="fl">按新答案确认得分（满分 {reviewCase.maxScore}）</span>
              <input
                aria-label={`${reviewCase.studentName} 新评分`}
                type="number"
                min={0}
                max={reviewCase.maxScore}
                step={0.5}
                value={teacherScore}
                onChange={(event) => setTeacherScore(Number(event.target.value))}
              />
            </label>
          ) : (
            <div className="impact-component-list">
              {reviewCase.targetComponents.map((component) => {
                const editor = components[component.sourcePublicId];
                return (
                  <div key={component.sourcePublicId} className="impact-component-row">
                    <div>
                      <b>{component.label}</b>
                      <span>满分 {component.maxScore}</span>
                    </div>
                    <input
                      aria-label={`${component.label} 得分`}
                      type="number"
                      min={0}
                      max={component.maxScore}
                      step={0.5}
                      value={editor.teacherScore}
                      onChange={(event) => setComponents((current) => ({
                        ...current,
                        [component.sourcePublicId]: {
                          ...current[component.sourcePublicId],
                          teacherScore: Number(event.target.value),
                        },
                      }))}
                    />
                    <input
                      aria-label={`${component.label} 学生答案证据`}
                      value={editor.evidenceText}
                      placeholder="给分时粘贴学生答案原句"
                      onChange={(event) => setComponents((current) => ({
                        ...current,
                        [component.sourcePublicId]: {
                          ...current[component.sourcePublicId],
                          evidenceText: event.target.value,
                        },
                      }))}
                    />
                    <input
                      aria-label={`${component.label} 备注`}
                      value={editor.teacherNote}
                      placeholder="可选备注"
                      onChange={(event) => setComponents((current) => ({
                        ...current,
                        [component.sourcePublicId]: {
                          ...current[component.sourcePublicId],
                          teacherNote: event.target.value,
                        },
                      }))}
                    />
                  </div>
                );
              })}
            </div>
          )}
          <label className="field">
            <span className="fl">本次复核说明（必填）</span>
            <input
              value={note}
              maxLength={300}
              placeholder="例如：按修订后的标准答案逐项核对"
              onChange={(event) => setNote(event.target.value)}
            />
          </label>
          <div className="impact-stage-action">
            <span>保存只产生新评分 revision，旧正式成绩和学习证据暂不改变。</span>
            <button
              className="primary"
              data-testid="impact-resolve-case"
              disabled={disabled || !canResolve}
              onClick={submitResolution}
            >
              确认新评分（暂不发布）
            </button>
          </div>
        </div>
      ) : reviewCase.state === "grade_confirmed" ? (
        <div className="impact-stage-action warning-box">
          <span>
            新评分 {reviewCase.resolvedTeacherScore} 分已保存；旧正式成绩仍然有效。
            {reviewCase.attemptState === "ready_to_publish"
              ? " 确认后将发布整份作业的新 revision。"
              : " 整份作业还有其他题待终审。"}
          </span>
          {reviewCase.attemptState === "ready_to_publish" ? (
            <button
              className="primary"
              data-testid="impact-publish-case"
              disabled={disabled}
              onClick={() => onPublished(reviewCase)}
            >
              明确发布整份新成绩
            </button>
          ) : (
            <button disabled={disabled} onClick={onOpenExam}>回批改台完成其余题目</button>
          )}
        </div>
      ) : (
        <div className="ok-banner">
          新评分已经明确发布；旧发布快照保留审计，正式学习证据已按新版本切换。
        </div>
      )}
      <p>{reviewCase.nextStepNote}</p>
    </article>
  );
}

function QuestionPerformancePanel({ onOpenExam }: { onOpenExam: () => void }) {
  const [catalog, setCatalog] = useState<QuestionPerformanceCatalog | null>(null);
  const [selected, setSelected] = useState<QuestionPerformanceItem | null>(null);
  const [preview, setPreview] = useState<QuestionVersionImpactPreview | null>(null);
  const [action, setAction] = useState<QuestionImpactAction>("future_only");
  const [plan, setPlan] = useState<QuestionImpactPlan | null>(null);
  const [defaultUpgrades, setDefaultUpgrades] = useState<Record<string, AssessmentDefaultUpgrade>>({});
  const [reviewCases, setReviewCases] = useState<QuestionImpactReviewCase[]>([]);
  const [caseBoundary, setCaseBoundary] = useState("");
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    setLoading(true);
    setError("");
    try {
      setCatalog(await loadQuestionPerformance(200));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const openImpact = async (item: QuestionPerformanceItem) => {
    setSelected(item);
    setPreview(null);
    setPlan(null);
    setDefaultUpgrades({});
    setReviewCases([]);
    setCaseBoundary("");
    setAction("future_only");
    setWorking(true);
    setError("");
    try {
      setPreview(await previewQuestionImpact(item.questionVersionPublicId));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const confirmImpact = async () => {
    if (!preview) return;
    setWorking(true);
    setError("");
    try {
      const nextPlan = await confirmQuestionImpact({
        requestKey: newImpactRequestKey(),
        questionVersionPublicId: preview.questionVersionPublicId,
        expectedPreviewHash: preview.previewHash,
        action,
        plannedBy: "local_teacher",
      });
      setPlan(nextPlan);
      if (nextPlan.taskCount > 0) {
        const existing = await loadQuestionImpactCases(nextPlan.publicId);
        setReviewCases(existing.cases);
        setCaseBoundary(existing.boundaryNote);
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const prepareCases = async () => {
    if (!plan || plan.taskCount === 0) return;
    setWorking(true);
    setError("");
    try {
      const result = await prepareQuestionImpactCases({
        planPublicId: plan.publicId,
        expectedTaskCount: plan.taskCount,
        preparedBy: "local_teacher",
      });
      setReviewCases(result.cases);
      setCaseBoundary(
        "待处理 case 已冻结旧评分/发布证据与目标版本；当前成绩、发布结果和学习证据均未改变。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const refreshCases = async (planPublicId: string) => {
    const next = await loadQuestionImpactCases(planPublicId);
    setReviewCases(next.cases);
    setCaseBoundary(next.boundaryNote);
  };

  const resolveCase = async (
    reviewCase: QuestionImpactReviewCase,
    teacherScore: number | null,
    components: Array<{
      sourcePublicId: string;
      teacherScore: number;
      evidenceText: string | null;
      teacherNote: string | null;
    }>,
    note: string,
  ) => {
    setWorking(true);
    setError("");
    try {
      await resolveQuestionImpactCase({
        requestKey: `${newImpactRequestKey()}-resolve`,
        casePublicId: reviewCase.publicId,
        expectedSourceSnapshotHash: reviewCase.sourceSnapshotHash,
        teacherScore,
        components,
        teacherNote: note,
        resolvedBy: "local_teacher",
      });
      await refreshCases(reviewCase.planPublicId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const publishCase = async (reviewCase: QuestionImpactReviewCase) => {
    if (!reviewCase.resolvedGradeDecisionPublicId) return;
    setWorking(true);
    setError("");
    try {
      await publishQuestionImpactCase({
        casePublicId: reviewCase.publicId,
        expectedGradeDecisionPublicId: reviewCase.resolvedGradeDecisionPublicId,
        publishedBy: "local_teacher",
      });
      await refreshCases(reviewCase.planPublicId);
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const upgradeFutureDefault = async (row: QuestionVersionImpactPreview["rows"][number]) => {
    if (!plan || !row.isCurrentDefault) return;
    setWorking(true);
    setError("");
    try {
      const upgraded = await upgradeAssessmentDefaultFromImpact({
        requestKey: `${newImpactRequestKey()}-future-default`,
        planPublicId: plan.publicId,
        sourceAssessmentVersionPublicId: row.assessmentVersionPublicId,
        expectedCurrentDefaultVersionPublicId: row.assessmentVersionPublicId,
        upgradedBy: "local_teacher",
      });
      setDefaultUpgrades((current) => ({
        ...current,
        [row.assessmentVersionPublicId]: upgraded,
      }));
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  return (
    <div className="question-performance-page">
      {error && <div className="error">{error}</div>}
      <section className="dashboard-panel question-performance-intro">
        <div className="dashboard-panel-head">
          <div>
            <b>题目实际表现</b>
            <span>只统计当前已发布成绩中老师确认的结果，不把单个班级正确率写成题目永久难度。</span>
          </div>
          <button disabled={loading || working} onClick={refresh}>刷新统计</button>
        </div>
        {catalog && <div className="hint">{catalog.boundaryNote}</div>}
      </section>

      {loading ? (
        <div className="loading">正在汇总题目表现…</div>
      ) : !catalog?.items.length ? (
        <section className="dashboard-panel empty-state">
          <b>还没有可汇总的正式题目</b>
          <span>题目被作业使用后会进入这里；发布老师确认的成绩后才会出现实际得分表现。</span>
        </section>
      ) : (
        <div className="question-performance-grid">
          {catalog.items.map((item) => (
            <article className="question-performance-card" key={item.questionVersionPublicId}>
              <div className="question-performance-card-head">
                <div>
                  <span className="tag">{TYPE_LABEL[item.questionType]}</span>
                  <span className="tag subtle">第 {item.revision} 版 · {item.qualityLevel}</span>
                </div>
                {item.hasVersionUpdateImpact && <span className="tag warning">有新版影响</span>}
              </div>
              <h3>{item.stem}</h3>
              <div className="question-performance-metrics">
                <div><b>{percentage(item.averageScoreRate)}</b><span>平均得分率</span></div>
                <div><b>{item.publishedResponseCount}</b><span>已发布作答</span></div>
                <div><b>{item.assessmentUsageCount}</b><span>使用作业数</span></div>
                <div><b>{percentage(item.fullCreditRate)}</b><span>满分率</span></div>
              </div>
              <div className="performance-score-split">
                <span>满分 {item.fullCreditCount}</span>
                <span>部分得分 {item.partialCreditCount}</span>
                <span>零分 {item.zeroScoreCount}</span>
                <span>首次 {item.firstAttemptCount} / 订正 {item.correctionAttemptCount}</span>
              </div>
              {item.contextBreakdown.length > 0 && (
                <div className="performance-contexts">
                  {item.contextBreakdown.map((context) => (
                    <span key={context.assessmentContext}>
                      {ASSESSMENT_CONTEXT_LABEL[context.assessmentContext] ?? context.assessmentContext}
                      {" "}{context.publishedResponseCount} 份 · {percentage(context.averageScoreRate)}
                    </span>
                  ))}
                </div>
              )}
              {item.latestPublishedAt && (
                <div className="muted">最近正式成绩：{new Date(item.latestPublishedAt).toLocaleString()}</div>
              )}
              {item.hasVersionUpdateImpact ? (
                <button className="primary" disabled={working} onClick={() => openImpact(item)}>
                  查看新版影响
                </button>
              ) : (
                <div className="ok-inline">当前作业均使用最新确认版本</div>
              )}
            </article>
          ))}
        </div>
      )}

      {selected && (
        <section className="dashboard-panel question-impact-panel">
          <div className="dashboard-panel-head">
            <div>
              <b>版本变更影响预览</b>
              <span>{selected.stem}</span>
            </div>
            <button disabled={working} onClick={() => {
              setSelected(null);
              setPreview(null);
              setPlan(null);
              setDefaultUpgrades({});
              setReviewCases([]);
              setCaseBoundary("");
            }}>关闭</button>
          </div>
          {working && !preview ? <div className="loading">正在核对历史使用范围…</div> : preview && (
            <>
              <div className="impact-target">
                <span>目标答案第 {preview.target.answerKeyRevision} 版</span>
                <span>目标评分点第 {preview.target.rubricRevision} 版</span>
                <span>目标知识链接第 {preview.target.linkSetRevision} 版</span>
              </div>
              <div className="question-impact-summary">
                <div><b>{preview.affectedAssessmentCount}</b><span>受影响作业</span></div>
                <div><b>{preview.unpublishedAttemptCount}</b><span>未发布作答</span></div>
                <div><b>{preview.publishedAttemptCount}</b><span>已发布作答</span></div>
                <div><b>{preview.activeLearningEvidenceCount}</b><span>有效学习证据</span></div>
                <div><b>{preview.profileSnapshotCount}</b><span>已有图谱引用</span></div>
              </div>
              <div className="impact-row-list">
                {preview.rows.map((row) => (
                  <article key={row.assessmentItemPublicId}>
                    <div>
                      <b>{row.assessmentTitle}</b>
                      <span>
                        {row.className} · 作业第 {row.assessmentVersionRevision} 版
                        {" · "}变化：{changeLabels(row)}
                      </span>
                    </div>
                    <div>
                      <span>未发布 {row.unpublishedAttemptCount}</span>
                      <span>已发布 {row.publishedAttemptCount}</span>
                      <span>证据 {row.activeLearningEvidenceCount}</span>
                      <span>图谱 {row.profileSnapshotCount}</span>
                    </div>
                    {plan && row.isCurrentDefault && (
                      defaultUpgrades[row.assessmentVersionPublicId] ? (
                        <div className="impact-default-result" data-testid="impact-default-upgraded">
                          <b>
                            未来上传默认第{" "}
                            {defaultUpgrades[row.assessmentVersionPublicId].defaultRevision} 版
                          </b>
                          <span>旧作业仍保留第 {row.assessmentVersionRevision} 版</span>
                        </div>
                      ) : (
                        <button
                          className="primary"
                          data-testid="impact-upgrade-default"
                          disabled={working}
                          onClick={() => upgradeFutureDefault(row)}
                        >
                          生成新版并用于以后上传
                        </button>
                      )
                    )}
                    {plan && !row.isCurrentDefault && (
                      <span className="tag subtle">历史版本保留，不切换</span>
                    )}
                  </article>
                ))}
              </div>
              <div className="impact-actions">
                <label className={action === "future_only" ? "selected" : ""}>
                  <input type="radio" checked={action === "future_only"} onChange={() => setAction("future_only")} />
                  <b>只用于以后新作业</b>
                  <span>不建立历史复核任务</span>
                </label>
                <label className={action === "recalculate_unpublished" ? "selected" : ""}>
                  <input
                    type="radio"
                    checked={action === "recalculate_unpublished"}
                    disabled={preview.unpublishedAttemptCount === 0}
                    onChange={() => setAction("recalculate_unpublished")}
                  />
                  <b>复核未发布作答</b>
                  <span>建立 {preview.unpublishedAttemptCount} 条待重新计算清单</span>
                </label>
                <label className={action === "review_published" ? "selected" : ""}>
                  <input
                    type="radio"
                    checked={action === "review_published"}
                    disabled={preview.publishedAttemptCount === 0}
                    onChange={() => setAction("review_published")}
                  />
                  <b>复核已发布成绩</b>
                  <span>建立 {preview.publishedAttemptCount} 条人工复核清单</span>
                </label>
              </div>
              <div className="warning-box">
                确认只冻结影响计划和待办清单，不会切换作业版本，不会改分、重新发布、改写学习证据或覆盖旧图谱。
              </div>
              {plan ? (
                <>
                  <div className="ok-banner">
                    已冻结处理计划，共 {plan.taskCount} 条待办；本次没有修改任何成绩和学习证据。
                  </div>
                  {preview.rows.some((row) => row.isCurrentDefault) && (
                    <div className="hint">
                      还需对当前默认作业点击“生成新版并用于以后上传”；系统不会把历史作业重绑到新版。
                    </div>
                  )}
                  {plan.taskCount > 0 && reviewCases.length === 0 && (
                    <div className="impact-prepare-row">
                      <div>
                        <b>下一步：建立逐份待处理记录</b>
                        <span>冻结每名学生当前评分或正式发布快照，供老师后续逐条重评。</span>
                      </div>
                      <button
                        className="primary"
                        data-testid="impact-prepare-cases"
                        disabled={working}
                        onClick={prepareCases}
                      >
                        {working ? "正在建立…" : `建立 ${plan.taskCount} 条待处理`}
                      </button>
                    </div>
                  )}
                  {reviewCases.length > 0 && (
                    <div className="impact-case-section" data-testid="impact-case-list">
                      <div className="impact-case-section-head">
                        <b>待处理记录 {reviewCases.length} 条</b>
                        <span>先确认新评分，再明确发布；两个动作不会被系统合并。</span>
                      </div>
                      <div className="impact-case-list">
                        {reviewCases.map((reviewCase) => (
                          <ImpactReviewCaseCard
                            key={reviewCase.publicId}
                            reviewCase={reviewCase}
                            disabled={working}
                            onResolved={resolveCase}
                            onPublished={publishCase}
                            onOpenExam={onOpenExam}
                          />
                        ))}
                      </div>
                      {caseBoundary && <div className="hint">{caseBoundary}</div>}
                    </div>
                  )}
                </>
              ) : (
                <button className="primary impact-confirm" disabled={working} onClick={confirmImpact}>
                  {working ? "正在冻结计划…" : "确认处理方式"}
                </button>
              )}
              <div className="hint">{preview.boundaryNote}</div>
            </>
          )}
        </section>
      )}
    </div>
  );
}

function BlueprintPaperEditorPanel({
  assembly,
  onClose,
}: {
  assembly: BlueprintAssembly;
  onClose: () => void;
}) {
  const [editor, setEditor] = useState<BlueprintPaperEditor | null>(null);
  const [title, setTitle] = useState(assembly.title);
  const [items, setItems] = useState<BlueprintPaperEditorItem[]>([]);
  const [edition, setEdition] = useState<BlueprintPaperEdition | null>(null);
  const [working, setWorking] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const loadEditor = async () => {
    const value = await loadBlueprintPaperEditor(assembly.public_id);
    setEditor(value);
    setTitle(value.title);
    setItems(value.items);
    return value;
  };

  useEffect(() => {
    let current = true;
    setWorking(true);
    loadBlueprintPaperEditor(assembly.public_id)
      .then((value) => {
        if (!current) return;
        setEditor(value);
        setTitle(value.title);
        setItems(value.items);
      })
      .catch((reason) => {
        if (current) setError(String(reason));
      })
      .finally(() => {
        if (current) setWorking(false);
      });
    return () => {
      current = false;
    };
  }, [assembly.public_id]);

  const moveItem = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= items.length) return;
    setItems((current) => {
      const next = [...current];
      [next[index], next[target]] = [next[target], next[index]];
      if (next[0].page_break_before) {
        next[0] = { ...next[0], page_break_before: false };
      }
      return next;
    });
    setEdition(null);
    setNotice("");
  };

  const changeQuestion = (index: number, questionVersionPublicId: string) => {
    const candidate = editor?.candidates.find(
      (item) => item.question_version_public_id === questionVersionPublicId,
    );
    if (!candidate) return;
    setItems((current) => current.map((item, currentIndex) => (
      currentIndex === index
        ? {
          ...item,
          question_version_public_id: candidate.question_version_public_id,
          question_type: candidate.question_type,
          stem: candidate.stem,
          material_text: candidate.material_text,
          score: candidate.score,
        }
        : item
    )));
    setEdition(null);
    setNotice("");
  };

  const selectedIds = new Set(items.map((item) => item.question_version_public_id));
  const canConfirm = Boolean(
    editor
    && title.trim()
    && items.length === editor.items.length
    && selectedIds.size === items.length,
  );

  const confirmPaper = async () => {
    if (!editor || !canConfirm) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const created = await confirmBlueprintPaper({
        requestKey: newPaperRequestKey(),
        assemblyPublicId: editor.assembly_public_id,
        expectedSourceAssessmentVersionPublicId:
          editor.source_assessment_version_public_id,
        title: title.trim(),
        items: items.map((item) => ({
          sourceSlotOrderIndex: item.source_slot_order_index,
          questionVersionPublicId: item.question_version_public_id,
          pageBreakBefore: item.page_break_before,
        })),
      });
      setEdition(created);
      setNotice(`第 ${created.revision} 版已冻结。现在可分别保存题卷和答案卷。`);
      await loadEditor();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const exportPaper = async (kind: "question" | "answer") => {
    if (!edition) return;
    setWorking(true);
    setError("");
    try {
      const outputPath = await save({
        defaultPath: kind === "question"
          ? edition.suggested_question_file_name
          : edition.suggested_answer_file_name,
        filters: [{ name: "可打印网页", extensions: ["html"] }],
      });
      if (!outputPath) return;
      const written = await writeBlueprintPaper(edition.public_id, kind, outputPath);
      setNotice(`已保存 ${written.file_name}。文件只在本机生成，不会自动布置或上传。`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  return (
    <section className="dashboard-panel blueprint-paper-editor">
      <div className="dashboard-panel-head">
        <div>
          <b>调整题目并生成打印稿</b>
          <span>
            {editor
              ? `当前基于第 ${editor.current_revision || 0} 版；每次确认都会新增一版`
              : "正在读取已冻结蓝图"}
          </span>
        </div>
        <button onClick={onClose}>关闭</button>
      </div>
      {error && <div className="error">{error}</div>}
      {notice && <div className="ok-banner">{notice}</div>}
      {working && !editor ? <div className="loading">正在准备换题与排版…</div> : editor && (
        <>
          <label className="field blueprint-paper-title">
            <span className="fl">试卷名称</span>
            <input
              value={title}
              maxLength={100}
              onChange={(event) => {
                setTitle(event.target.value);
                setEdition(null);
              }}
            />
          </label>
          <div className="hint">{editor.boundary_note}</div>
          <div className="blueprint-paper-items">
            {items.map((item, index) => {
              const compatible = editor.candidates.filter((candidate) => (
                candidate.question_type === item.question_type
                && sameNumber(candidate.score, item.score)
                && (
                  candidate.question_version_public_id === item.question_version_public_id
                  || !selectedIds.has(candidate.question_version_public_id)
                )
              ));
              const currentListed = compatible.some(
                (candidate) =>
                  candidate.question_version_public_id === item.question_version_public_id,
              );
              return (
                <article className="blueprint-paper-item" key={item.source_slot_order_index}>
                  <div className="blueprint-paper-order">
                    <b>第 {index + 1} 题</b>
                    <div>
                      <button
                        aria-label={`第 ${index + 1} 题上移`}
                        disabled={index === 0}
                        onClick={() => moveItem(index, -1)}
                      >
                        上移
                      </button>
                      <button
                        aria-label={`第 ${index + 1} 题下移`}
                        disabled={index === items.length - 1}
                        onClick={() => moveItem(index, 1)}
                      >
                        下移
                      </button>
                    </div>
                  </div>
                  <label className="field">
                    <span className="fl">{TYPE_LABEL[item.question_type]} · {item.score} 分</span>
                    <select
                      aria-label={`第 ${index + 1} 题换题`}
                      value={item.question_version_public_id}
                      onChange={(event) => changeQuestion(index, event.target.value)}
                    >
                      {!currentListed && (
                        <option value={item.question_version_public_id}>
                          {item.stem}（沿用冻结版本）
                        </option>
                      )}
                      {compatible.map((candidate) => (
                        <option
                          key={candidate.question_version_public_id}
                          value={candidate.question_version_public_id}
                        >
                          {candidate.stem}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p>{item.stem}</p>
                  <label className="blueprint-paper-break">
                    <input
                      type="checkbox"
                      disabled={index === 0}
                      checked={index > 0 && item.page_break_before}
                      onChange={(event) => {
                        setItems((current) => current.map((currentItem, currentIndex) => (
                          currentIndex === index
                            ? { ...currentItem, page_break_before: event.target.checked }
                            : currentItem
                        )));
                        setEdition(null);
                      }}
                    />
                    从新页开始
                  </label>
                </article>
              );
            })}
          </div>
          <div className="blueprint-confirm-row">
            <div>
              <b>{canConfirm ? "可以生成新的打印版" : "请避免重复题目"}</b>
              <span>题卷不含答案；答案卷包含冻结答案和评分点。未来上传默认版不会自动改变。</span>
            </div>
            <button className="primary" disabled={working || !canConfirm} onClick={confirmPaper}>
              {working ? "正在冻结…" : "确认新版并生成打印稿"}
            </button>
          </div>
          {edition && (
            <div className="blueprint-paper-export">
              <div>
                <b>第 {edition.revision} 版已就绪</b>
                <span>{edition.items.length} 道 · 两份文件分别保存</span>
              </div>
              <button disabled={working} onClick={() => exportPaper("question")}>保存题卷</button>
              <button disabled={working} onClick={() => exportPaper("answer")}>保存答案卷</button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

export default function QuestionBank({ onOpenExam }: { onOpenExam: () => void }) {
  const [mode, setMode] = useState<"import" | "search" | "candidates" | "performance" | "blueprint">("import");
  const [options, setOptions] = useState<BlueprintOptions | null>(null);
  const [classId, setClassId] = useState(0);
  const [mapPublicId, setMapPublicId] = useState("");
  const [curriculumPublicId, setCurriculumPublicId] = useState("");
  const [title, setTitle] = useState("课堂练习");
  const [totalScore, setTotalScore] = useState(5);
  const [targets, setTargets] = useState<Record<K1QuestionType, number>>({
    single: 5,
    multiple: 0,
    true_false: 0,
    fill_blank: 0,
    short_answer: 0,
  });
  const [requiredKnowledge, setRequiredKnowledge] = useState<string[]>([]);
  const [preview, setPreview] = useState<BlueprintPreview | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [history, setHistory] = useState<BlueprintAssembly[]>([]);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [paperAssembly, setPaperAssembly] = useState<BlueprintAssembly | null>(null);

  useEffect(() => {
    loadBlueprintOptions()
      .then((value) => {
        setOptions(value);
        setClassId(value.classes[0]?.id ?? 0);
        setMapPublicId(value.knowledge_maps[0]?.public_id ?? "");
      })
      .catch((reason) => setError(String(reason)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!classId) {
      setHistory([]);
      return;
    }
    listBlueprintAssemblies(classId)
      .then(setHistory)
      .catch((reason) => setError(String(reason)));
  }, [classId]);

  const invalidatePreview = () => {
    setPreview(null);
    setSelected([]);
    setNotice("");
  };

  const curriculumOptions = useMemo(
    () => options?.curriculum_nodes.filter(
      (node) => node.knowledge_map_public_id === mapPublicId,
    ) ?? [],
    [options, mapPublicId],
  );

  const scopeCurriculumIds = useMemo(() => {
    if (!curriculumPublicId) return null;
    const result = new Set([curriculumPublicId]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const node of curriculumOptions) {
        if (node.parent_public_id && result.has(node.parent_public_id) && !result.has(node.public_id)) {
          result.add(node.public_id);
          changed = true;
        }
      }
    }
    return result;
  }, [curriculumOptions, curriculumPublicId]);

  const knowledgeOptions = useMemo(
    () => options?.knowledge_nodes.filter((node) => (
      node.knowledge_map_public_id === mapPublicId
      && (!scopeCurriculumIds
        || (node.curriculum_node_public_id
          ? scopeCurriculumIds.has(node.curriculum_node_public_id)
          : false))
    )) ?? [],
    [options, mapPublicId, scopeCurriculumIds],
  );

  const previewInput = useMemo<BlueprintPreviewInput>(() => ({
    classId,
    knowledgeMapPublicId: mapPublicId,
    curriculumNodePublicId: curriculumPublicId || null,
    totalScore,
    questionTypeTargets: TYPE_ORDER.map((questionType) => ({
      questionType,
      count: targets[questionType],
    })),
    requiredKnowledgeNodePublicIds: requiredKnowledge,
  }), [classId, mapPublicId, curriculumPublicId, totalScore, targets, requiredKnowledge]);

  const selectedCandidates = useMemo(() => {
    if (!preview) return [];
    const selectedIds = new Set(selected);
    return preview.candidates.filter(
      (candidate) => selectedIds.has(candidate.question_version_public_id),
    );
  }, [preview, selected]);

  const liveCounts = useMemo(() => {
    const result = Object.fromEntries(TYPE_ORDER.map((type) => [type, 0])) as Record<K1QuestionType, number>;
    selectedCandidates.forEach((candidate) => {
      result[candidate.question_type] += 1;
    });
    return result;
  }, [selectedCandidates]);

  const liveScore = selectedCandidates.reduce((sum, candidate) => sum + candidate.score, 0);
  const coveredRequired = useMemo(() => {
    const covered = new Set<string>();
    selectedCandidates.forEach((candidate) => {
      candidate.knowledge_nodes.forEach((node) => covered.add(node.public_id));
    });
    return requiredKnowledge.filter((publicId) => covered.has(publicId));
  }, [selectedCandidates, requiredKnowledge]);
  const selectionReady = Boolean(
    preview?.can_confirm
    && title.trim()
    && sameNumber(liveScore, totalScore)
    && TYPE_ORDER.every((type) => liveCounts[type] === targets[type])
    && coveredRequired.length === requiredKnowledge.length,
  );

  const runPreview = async () => {
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const value = await previewBlueprint(previewInput);
      setPreview(value);
      setSelected(value.recommended_question_version_public_ids);
      if (value.can_confirm) {
        setNotice(`找到 ${value.candidates.length} 道合格题，已按蓝图初选。`);
      }
    } catch (reason) {
      setPreview(null);
      setSelected([]);
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  const toggleCandidate = (candidate: BlueprintCandidate) => {
    setSelected((current) => (
      current.includes(candidate.question_version_public_id)
        ? current.filter((value) => value !== candidate.question_version_public_id)
        : [...current, candidate.question_version_public_id]
    ));
    setNotice("");
  };

  const confirm = async () => {
    if (!preview || !selectionReady) return;
    setWorking(true);
    setError("");
    try {
      const created = await confirmBlueprint({
        requestKey: newRequestKey(),
        title: title.trim(),
        preview: previewInput,
        expectedPreviewHash: preview.preview_hash,
        selectedQuestionVersionPublicIds: selected,
      });
      setNotice(`已冻结“${created.title}”：${created.items.length} 道，${created.total_score} 分。`);
      setHistory(await listBlueprintAssemblies(classId));
      setPreview(null);
      setSelected([]);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  };

  if (loading) return <div className="page"><div className="loading">正在读取题库…</div></div>;

  return (
    <div className="page blueprint-page">
      <div className="page-head">
        <div>
          <h1>题目与知识库</h1>
          <p className="sub">上传现成题目、找题、处理批改形成的新题候选，或按教材范围组一份可批改作业。</p>
        </div>
        <button onClick={onOpenExam}>返回题目批改</button>
      </div>

      <div className="tabs question-bank-tabs">
        <button className={mode === "import" ? "tab active" : "tab"} onClick={() => setMode("import")}>
          导入题目
        </button>
        <button className={mode === "search" ? "tab active" : "tab"} onClick={() => setMode("search")}>
          找题与查重
        </button>
        <button className={mode === "candidates" ? "tab active" : "tab"} onClick={() => setMode("candidates")}>
          待整理新题
        </button>
        <button className={mode === "performance" ? "tab active" : "tab"} onClick={() => setMode("performance")}>
          表现与版本影响
        </button>
        <button className={mode === "blueprint" ? "tab active" : "tab"} onClick={() => setMode("blueprint")}>
          按蓝图组卷
        </button>
      </div>

      {mode === "import" ? <SourceImportPanel /> : mode === "search" ? <QuestionSearchPanel options={options} /> : mode === "candidates" ? (
        <CandidateReviewPanel />
      ) : mode === "performance" ? (
        <QuestionPerformancePanel onOpenExam={onOpenExam} />
      ) : (
        <>
          {error && <div className="error">{error}</div>}
          {notice && <div className="ok-banner">{notice}</div>}

          <section className="dashboard-panel blueprint-step">
        <div className="dashboard-panel-head">
          <div><b>1. 这次要练什么</b><span>只保留日常需要的几个选择</span></div>
          <span className="tag">蓝图</span>
        </div>
        {!options?.classes.length || !options.knowledge_maps.length ? (
          <div className="hint">
            需要先建立班级和已确认教材知识地图。题目也必须达到“可用于图谱（L3）”后才能组卷。
          </div>
        ) : (
          <>
            <div className="blueprint-fields">
              <label className="field">
                <span className="fl">班级</span>
                <select value={classId} onChange={(event) => {
                  setClassId(Number(event.target.value));
                  invalidatePreview();
                }}>
                  {options.classes.map((item) => (
                    <option key={item.id} value={item.id}>{item.name}{item.term ? ` · ${item.term}` : ""}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span className="fl">教材</span>
                <select value={mapPublicId} onChange={(event) => {
                  setMapPublicId(event.target.value);
                  setCurriculumPublicId("");
                  setRequiredKnowledge([]);
                  invalidatePreview();
                }}>
                  {options.knowledge_maps.map((item) => (
                    <option key={item.public_id} value={item.public_id}>{item.title} · 第 {item.revision} 版</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span className="fl">范围</span>
                <select value={curriculumPublicId} onChange={(event) => {
                  setCurriculumPublicId(event.target.value);
                  setRequiredKnowledge([]);
                  invalidatePreview();
                }}>
                  <option value="">全册</option>
                  {curriculumOptions.map((item) => (
                    <option key={item.public_id} value={item.public_id}>{item.title}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span className="fl">组卷名称</span>
                <input value={title} maxLength={100} onChange={(event) => setTitle(event.target.value)} />
              </label>
            </div>

            <div className="blueprint-type-row">
              {TYPE_ORDER.map((type) => (
                <label className="field" key={type}>
                  <span className="fl">{TYPE_LABEL[type]}</span>
                  <input
                    type="number"
                    min={0}
                    max={50}
                    value={targets[type]}
                    onChange={(event) => {
                      setTargets((current) => ({
                        ...current,
                        [type]: Math.max(0, Math.min(50, Number(event.target.value) || 0)),
                      }));
                      invalidatePreview();
                    }}
                  />
                </label>
              ))}
              <label className="field blueprint-score-field">
                <span className="fl">总分</span>
                <input type="number" min={0.001} max={500} step={0.5} value={totalScore} onChange={(event) => {
                  setTotalScore(Number(event.target.value));
                  invalidatePreview();
                }} />
              </label>
            </div>

            <div className="blueprint-knowledge">
              <span>必须覆盖的知识点（可选）</span>
              <p>不选时，系统仍只会使用当前教材范围内的题；选择后会强制至少有一道题覆盖它。</p>
              <div>
                {knowledgeOptions.map((knowledge) => (
                  <button
                    key={knowledge.public_id}
                    className={requiredKnowledge.includes(knowledge.public_id) ? "chip on" : "chip"}
                    onClick={() => {
                      setRequiredKnowledge((current) => (
                        current.includes(knowledge.public_id)
                          ? current.filter((value) => value !== knowledge.public_id)
                          : [...current, knowledge.public_id]
                      ));
                      invalidatePreview();
                    }}
                  >
                    {knowledge.title}
                  </button>
                ))}
                {!knowledgeOptions.length && <span className="muted">当前范围还没有可选知识点。</span>}
              </div>
            </div>

            <button
              className="primary"
              disabled={working || !classId || !mapPublicId}
              onClick={runPreview}
            >
              {working ? "正在整理…" : "整理可用题目"}
            </button>
          </>
        )}
          </section>

          {preview && (
            <section className="dashboard-panel blueprint-step">
          <div className="dashboard-panel-head">
            <div><b>2. 只检查系统初选</b><span>{preview.boundary_note}</span></div>
            <span className={preview.can_confirm ? "tag ok" : "tag warn"}>
              {preview.can_confirm ? `${preview.candidates.length} 道可选` : "需要补题"}
            </span>
          </div>
          {preview.blockers.map((message) => <div className="error" key={message}>{message}</div>)}
          {preview.warnings.map((message) => <div className="exam-notice" key={message}>{message}</div>)}

          <div className={selectionReady ? "blueprint-live ready" : "blueprint-live"}>
            <b>当前选中 {selectedCandidates.length} 道 · {liveScore.toFixed(1)} / {totalScore} 分</b>
            <span>
              {TYPE_ORDER.filter((type) => targets[type] > 0).map(
                (type) => `${TYPE_LABEL[type]} ${liveCounts[type]}/${targets[type]}`,
              ).join("　")}
            </span>
            {requiredKnowledge.length > 0 && (
              <span>必覆盖知识点 {coveredRequired.length}/{requiredKnowledge.length}</span>
            )}
          </div>

          <div className="blueprint-candidates">
            {preview.candidates.map((candidate) => {
              const checked = selected.includes(candidate.question_version_public_id);
              return (
                <label className={checked ? "blueprint-candidate selected" : "blueprint-candidate"} key={candidate.question_version_public_id}>
                  <input type="checkbox" checked={checked} onChange={() => toggleCandidate(candidate)} />
                  <div>
                    <div className="blueprint-candidate-head">
                      <span className="tag">{TYPE_LABEL[candidate.question_type]}</span>
                      <b>{candidate.stem}</b>
                      <strong>{candidate.score} 分</strong>
                    </div>
                    <p>{candidate.explanation}</p>
                    <div className="blueprint-evidence-tags">
                      {candidate.knowledge_nodes.map((node) => <span key={`${node.public_id}-${node.relation_type}`}>知识 · {node.title}</span>)}
                      {candidate.ability_dimensions.map((ability) => <span key={`${ability.public_id}-${ability.response_mode}`}>能力 · {ability.title}</span>)}
                    </div>
                  </div>
                </label>
              );
            })}
          </div>

          <div className="blueprint-confirm-row">
            <div>
              <b>{selectionReady ? "蓝图已对齐，可以确认" : "请让题型数量、总分和必覆盖知识点全部对齐"}</b>
              <span>确认后题目、答案、评分规则和知识链接版本都会冻结。</span>
            </div>
            <button className="primary" disabled={working || !selectionReady} onClick={confirm}>
              确认并建立作业
            </button>
          </div>
            </section>
          )}

          {paperAssembly && (
            <BlueprintPaperEditorPanel
              assembly={paperAssembly}
              onClose={() => setPaperAssembly(null)}
            />
          )}

          <section className="dashboard-panel blueprint-history">
        <div className="dashboard-panel-head">
          <div><b>最近确认的组卷</b><span>这里是不可变蓝图记录，不代表学生已经提交或成绩已经发布</span></div>
        </div>
        {!history.length ? (
          <div className="muted">当前班级还没有确认过组卷。</div>
        ) : history.map((assembly) => (
          <div className="blueprint-history-row" key={assembly.public_id}>
            <div>
              <b>{assembly.title}</b>
              <span>{assembly.knowledge_map_title}{assembly.curriculum_node_title ? ` · ${assembly.curriculum_node_title}` : ""}</span>
            </div>
            <span>{assembly.items.length} 道 · {assembly.total_score} 分</span>
            <time>{new Date(assembly.confirmed_at).toLocaleString()}</time>
            <button onClick={() => setPaperAssembly(assembly)}>换题与打印</button>
          </div>
        ))}
          </section>
        </>
      )}
    </div>
  );
}
