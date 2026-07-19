import { useEffect, useMemo, useState } from "react";
import {
  BlueprintAssembly,
  BlueprintCandidate,
  BlueprintOptions,
  BlueprintPreview,
  BlueprintPreviewInput,
  CandidateReviewItem,
  DuplicateCandidate,
  K1QuestionType,
  QuestionSearchInput,
  QuestionSearchResponse,
  confirmBlueprint,
  discardCandidate,
  listBlueprintAssemblies,
  loadCandidateReviewInbox,
  loadBlueprintOptions,
  previewBlueprint,
  promoteCandidateToL1,
  reviewDuplicate,
  searchQuestions,
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

function sameNumber(left: number, right: number) {
  return Math.abs(left - right) < 0.000001;
}

function duplicateLabel(candidate: DuplicateCandidate) {
  if (candidate.decision === "same_family") return "老师已归为同题变式";
  if (candidate.decision === "independent") return "老师已确认是不同题";
  return candidate.match_kind === "exact" ? "内容完全一致" : "疑似同题变式";
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

export default function QuestionBank({ onOpenExam }: { onOpenExam: () => void }) {
  const [mode, setMode] = useState<"search" | "candidates" | "blueprint">("search");
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
          <p className="sub">找题、处理批改解析形成的新题候选，或按教材范围直接组一份可批改的作业。</p>
        </div>
        <button onClick={onOpenExam}>返回题目批改</button>
      </div>

      <div className="tabs question-bank-tabs">
        <button className={mode === "search" ? "tab active" : "tab"} onClick={() => setMode("search")}>
          找题与查重
        </button>
        <button className={mode === "candidates" ? "tab active" : "tab"} onClick={() => setMode("candidates")}>
          待整理新题
        </button>
        <button className={mode === "blueprint" ? "tab active" : "tab"} onClick={() => setMode("blueprint")}>
          按蓝图组卷
        </button>
      </div>

      {mode === "search" ? <QuestionSearchPanel options={options} /> : mode === "candidates" ? (
        <CandidateReviewPanel />
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
          </div>
        ))}
          </section>
        </>
      )}
    </div>
  );
}
