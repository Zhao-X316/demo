import { useState, useMemo, useEffect } from "react";
import {
  SourceInboxItem,
  K1QuestionType,
  SourceDraft,
  loadSourceInbox,
  importAndAnalyzeSource,
  AcceptSourceDraftInput,
  acceptSourceDraft,
  discardSourceDraft,
} from "../../api/knowledge";
import { open } from "@tauri-apps/plugin-dialog";
import {
  sourceFileLabel,
  sourceTypeLabel,
  sourceFailureMessage,
  TYPE_LABEL,
  TYPE_ORDER,
} from "./shared";
import { AnswerSourcePanel } from "./AnswerSourcePanel";
import { KnowledgeLinkPanel } from "./KnowledgeLinkPanel";

function newSourceRequestKey(action: "import" | "accept" | "discard") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-source-${action}-${random}`;
}

export function SourceImportPanel() {
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
