import {
  BlueprintAssembly,
  BlueprintPaperEditor,
  BlueprintPaperEditorItem,
  BlueprintPaperEdition,
  loadBlueprintPaperEditor,
  confirmBlueprintPaper,
  writeBlueprintPaper,
} from "../../api/knowledge";
import { useState, useEffect } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { sameNumber, TYPE_LABEL } from "./shared";

function newPaperRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-blueprint-paper-${random}`;
}

export function BlueprintPaperEditorPanel({
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
