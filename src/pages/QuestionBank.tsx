import { useState, useEffect, useMemo } from "react";
import {
  BlueprintOptions,
  K1QuestionType,
  BlueprintPreview,
  BlueprintAssembly,
  loadBlueprintOptions,
  listBlueprintAssemblies,
  BlueprintPreviewInput,
  previewBlueprint,
  BlueprintCandidate,
  confirmBlueprint,
} from "../api/knowledge";
import { TYPE_ORDER, sameNumber, TYPE_LABEL } from "./question-bank/shared";
import { SourceImportPanel } from "./question-bank/SourceImportPanel";
import { QuestionSearchPanel } from "./question-bank/QuestionSearchPanel";
import { CandidateReviewPanel } from "./question-bank/CandidateReviewPanel";
import { QuestionPerformancePanel } from "./question-bank/QuestionPerformancePanel";
import { BlueprintPaperEditorPanel } from "./question-bank/BlueprintPaperEditorPanel";

function newRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-blueprint-${random}`;
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
