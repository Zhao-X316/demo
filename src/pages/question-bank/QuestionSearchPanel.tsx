import {
  DuplicateCandidate,
  BlueprintOptions,
  K1QuestionType,
  QuestionSearchInput,
  QuestionSearchResponse,
  SemanticQuestionSearchResponse,
  semanticSearchQuestions,
  searchQuestions,
  reviewDuplicate,
} from "../../api/knowledge";
import { useState, useMemo, useEffect } from "react";
import { TYPE_ORDER, TYPE_LABEL } from "./shared";

function newDuplicateRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-duplicate-${random}`;
}

function newSemanticSearchRequestKey() {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-semantic-search-${random}`;
}

function duplicateLabel(candidate: DuplicateCandidate) {
  if (candidate.decision === "same_family") return "老师已归为同题变式";
  if (candidate.decision === "independent") return "老师已确认是不同题";
  return candidate.match_kind === "exact" ? "内容完全一致" : "疑似同题变式";
}

export function QuestionSearchPanel({ options }: { options: BlueprintOptions | null }) {
  const [searchMode, setSearchMode] = useState<"keyword" | "semantic">("keyword");
  const [query, setQuery] = useState("");
  const [ownerScope, setOwnerScope] = useState<"all" | "personal" | "official">("all");
  const [questionType, setQuestionType] = useState<K1QuestionType | "">("");
  const [minimumQuality, setMinimumQuality] = useState<QuestionSearchInput["minimumQuality"]>("L0");
  const [duplicateOnly, setDuplicateOnly] = useState(false);
  const [mapPublicId, setMapPublicId] = useState("");
  const [curriculumPublicId, setCurriculumPublicId] = useState("");
  const [knowledgePublicId, setKnowledgePublicId] = useState("");
  const [result, setResult] = useState<QuestionSearchResponse | null>(null);
  const [semanticResult, setSemanticResult] = useState<SemanticQuestionSearchResponse | null>(null);
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
    duplicateOnly: searchMode === "keyword" && duplicateOnly,
    limit: searchMode === "semantic" ? 20 : 50,
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
    searchMode,
  ]);

  const runSearch = async () => {
    setWorking(true);
    setError("");
    setNotice("");
    try {
      if (searchMode === "semantic") {
        if (query.trim().length < 2) {
          throw new Error("按意思查找需要输入至少 2 个字符。");
        }
        const semantic = await semanticSearchQuestions({
          requestKey: newSemanticSearchRequestKey(),
          search: input,
        });
        setSemanticResult(semantic);
        if (semantic.failure) setError(semantic.failure.safe_message);
        setResult(null);
      } else {
        setResult(await searchQuestions(input));
        setSemanticResult(null);
      }
    } catch (reason) {
      setResult(null);
      setSemanticResult(null);
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
          <label className="field">
            <span className="fl">查找方式</span>
            <select
              value={searchMode}
              onChange={(event) => {
                const next = event.target.value as typeof searchMode;
                setSearchMode(next);
                if (next === "semantic") setDuplicateOnly(false);
                setResult(null);
                setSemanticResult(null);
                setError("");
              }}
            >
              <option value="keyword">关键词匹配（本机）</option>
              <option value="semantic">按意思查找（AI）</option>
            </select>
          </label>
          <label className="field question-search-keyword">
            <span className="fl">{searchMode === "semantic" ? "想找什么题" : "关键词"}</span>
            <input
              value={query}
              maxLength={100}
              placeholder={searchMode === "semantic"
                ? "如：找考查中国近代史开端、适合基础复习的题"
                : "题干、材料或选项，如：洋务运动"}
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
            {working
              ? searchMode === "semantic" ? "正在理解并查找…" : "正在查找…"
              : searchMode === "semantic" ? "按意思查找" : "查找题目"}
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
              <input
                type="checkbox"
                checked={duplicateOnly}
                disabled={searchMode === "semantic"}
                onChange={(event) => setDuplicateOnly(event.target.checked)}
              />
              只看有重复提示的题
            </label>
          </div>
        </details>
        {searchMode === "semantic" && (
          <div className="question-search-summary">
            <b>AI 只负责排序</b>
            <span>系统先在本机按权限和筛选条件冻结最多 80 道候选，再让 AI 按意思排序；不会自动合并、改答案或改变历史作业。</span>
          </div>
        )}
        {result && (
          <div className="question-search-summary">
            <b>找到 {result.total} 道题</b>
            <span>{result.boundary_note}</span>
          </div>
        )}
        {semanticResult && (
          <div className="question-search-summary">
            <b>
              {semanticResult.status === "succeeded"
                ? `按意思找到 ${semanticResult.items.length} 道题`
                : "本次按意思查找未完成"}
            </b>
            <span>{semanticResult.boundaryNote}</span>
          </div>
        )}
      </section>

      <section className="question-search-results">
        {semanticResult?.items.map(({ candidate, score, reason }) => (
          <article className="question-search-card" key={candidate.questionVersionPublicId}>
            <div className="question-search-card-head">
              <div>
                <span className="tag">{TYPE_LABEL[candidate.questionType]}</span>
                <span className="tag">{candidate.ownerLabel}</span>
                <span className="tag">{candidate.qualityLevel}</span>
                <span className="tag ok">语义相关 {Math.round(score * 100)}%</span>
              </div>
              <strong>{candidate.maxScore} 分</strong>
            </div>
            <h3>{candidate.stem}</h3>
            {candidate.materialText && <p>{candidate.materialText}</p>}
            {candidate.options.length > 0 && (
              <div className="question-search-options">
                {candidate.options.map((option) => (
                  <span key={option.label}>{option.label}. {option.content}</span>
                ))}
              </div>
            )}
            <div className="question-search-summary">
              <b>为什么匹配</b>
              <span>{reason}</span>
            </div>
            <div className="blueprint-evidence-tags">
              {candidate.knowledgeTitles.map((title) => (
                <span key={`knowledge-${title}`}>知识 · {title}</span>
              ))}
              {candidate.abilityTitles.map((title) => (
                <span key={`ability-${title}`}>能力 · {title}</span>
              ))}
            </div>
          </article>
        ))}
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
        {semanticResult
          && semanticResult.status === "succeeded"
          && semanticResult.items.length === 0 && (
          <div className="dashboard-panel muted">
            冻结候选中没有足够相关的题。可以换一种说法，或先选择教材、章节和题型缩小范围。
          </div>
        )}
      </section>
    </>
  );
}
