import {
  LinkSuggestionSource,
  K1QuestionType,
  LinkSuggestionInput,
  ConfirmedSourceLinksInput,
  loadLinkReviewCatalog,
  LinkReviewInboxItem,
  LinkSuggestionDraftView,
  loadLinkReviewInbox,
  loadLinkReviewEditor,
  suggestKnowledgeLinks,
  confirmKnowledgeLinks,
} from "../../api/knowledge";
import { useState, useEffect } from "react";
import { TYPE_LABEL } from "./shared";

function newLinkRequestKey(action: "suggest" | "confirm") {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `k1-link-${action}-${random}`;
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

export function KnowledgeLinkPanel() {
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
