import { useEffect, useState } from "react";
import { examSubjectiveLinkEditor, examSubjectiveLinkSave } from "../../api/exam";
import type { SubjectiveLinkEditor, SubjectiveSourceLinkInput } from "../../api/exam";

const KNOWLEDGE_RELATIONS = [
  ["direct_assessment", "直接考查"],
  ["answer_basis", "答案依据"],
  ["rubric_basis", "评分点依据"],
  ["context", "题干背景"],
] as const;

const ABILITY_RESPONSE_MODES = [
  ["recall", "回忆作答"],
  ["structured_response", "结构化表达"],
  ["source_analysis", "史料分析"],
  ["argumentation", "论证评价"],
  ["recognition", "识别选择"],
] as const;

export function SubjectiveLinkPanel({
  assessmentItemId,
  onDone,
  onError,
}: {
  assessmentItemId: number;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [editor, setEditor] = useState<SubjectiveLinkEditor | null>(null);
  const [sources, setSources] = useState<SubjectiveSourceLinkInput[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  async function loadEditor(itemId: number) {
    if (!itemId) return;
    setLoading(true);
    try {
      const next = await examSubjectiveLinkEditor(itemId);
      setEditor(next);
      setSources(next.sources.map((source) => ({
        source_type: source.source_type,
        source_public_id: source.source_public_id,
        knowledge_links: source.knowledge_links.map((link) => ({
          knowledge_node_id: link.knowledge_node_id,
          relation_type: link.relation_type,
        })),
        ability_links: source.ability_links.map((link) => ({
          ability_dimension_id: link.ability_dimension_id,
          evidence_strength: link.evidence_strength,
          response_mode: link.response_mode,
        })),
      })));
    } catch (err) {
      setEditor(null);
      setSources([]);
      onError(String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadEditor(assessmentItemId);
  }, [assessmentItemId]);

  function updateSource(index: number, update: (source: SubjectiveSourceLinkInput) => SubjectiveSourceLinkInput) {
    setSources((current) => current.map((source, sourceIndex) => sourceIndex === index ? update(source) : source));
  }

  function addKnowledge(sourceIndex: number) {
    const option = editor?.knowledge_options.find((candidate) => (
      !sources[sourceIndex]?.knowledge_links.some((link) => link.knowledge_node_id === candidate.id)
    ));
    if (!option) {
      onError("没有可继续添加的知识点；可先到知识库补充知识节点");
      return;
    }
    updateSource(sourceIndex, (source) => ({
      ...source,
      knowledge_links: [...source.knowledge_links, {
        knowledge_node_id: option.id,
        relation_type: source.source_type === "rubric_point" ? "rubric_basis" : "answer_basis",
      }],
    }));
  }

  function addAbility(sourceIndex: number) {
    const option = editor?.ability_options.find((candidate) => (
      !sources[sourceIndex]?.ability_links.some((link) => link.ability_dimension_id === candidate.id)
    ));
    if (!option) {
      onError("没有可继续添加的能力维度；可先到知识库补充能力维度");
      return;
    }
    updateSource(sourceIndex, (source) => ({
      ...source,
      ability_links: [...source.ability_links, {
        ability_dimension_id: option.id,
        evidence_strength: source.source_type === "rubric_point" ? 0.7 : 0.5,
        response_mode: source.source_type === "rubric_point" ? "structured_response" : "recall",
      }],
    }));
  }

  async function save() {
    if (!editor || !sources.length) return;
    if (!window.confirm(
      "确认这些知识点与能力链接？\n\n系统会另存新的链接集和未来作业版本；当前学生作答、成绩、已发布结果和历史图谱均不会改变。",
    )) return;
    setSaving(true);
    try {
      const result = await examSubjectiveLinkSave(assessmentItemId, sources);
      const prefix = result.outcome === "created_new_version"
        ? `已另存未来作业 v${result.adopted_assessment_revision}`
        : result.outcome === "already_saved"
          ? "相同链接此前已经保存"
          : "当前版本已是这些链接";
      onDone(`${prefix}：${result.knowledge_link_count} 条知识链接、${result.ability_link_count} 条能力链接；历史成绩保持不变`);
      await loadEditor(assessmentItemId);
    } catch (err) {
      onError(String(err));
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <section className="exam-card subjective-link-panel"><div className="meta">正在读取本题知识与能力链接…</div></section>;
  if (!editor) return null;
  const linkedCount = sources.reduce((total, source) => total + source.knowledge_links.length + source.ability_links.length, 0);
  return (
    <details className="exam-card subjective-link-panel" open={linkedCount === 0}>
      <summary>
        <b>本题知识与能力</b>
        <span className={linkedCount ? "tag pass" : "tag wait"}>{linkedCount ? `已确认 ${linkedCount} 条` : "待建立"}</span>
        <span className="meta">用于未来作业和学习图谱</span>
      </summary>
      <p className="meta">
        系统按第 {editor.question_no} 题的{editor.question_type === "fill_blank" ? "答案槽位" : "评分点"}记录链接。
        只有老师确认链接并发布老师终审成绩后，才形成正式图谱证据。
      </p>
      <div className="subjective-link-sources">
        {editor.sources.map((view, sourceIndex) => {
          const source = sources[sourceIndex];
          if (!source) return null;
          return (
            <article className="subjective-link-source" key={view.source_public_id}>
              <div className="exam-card-head">
                <div><b>{view.label}</b><div className="meta">满分 {view.max_score} · {view.stable_id}</div></div>
                <span className={source.knowledge_links.length ? "tag pass" : "tag wait"}>
                  {source.knowledge_links.length ? "已有知识点" : "未连知识点"}
                </span>
              </div>
              <div className="subjective-link-list">
                {source.knowledge_links.map((link, linkIndex) => (
                  <div className="subjective-link-row" key={`knowledge-${linkIndex}`}>
                    <select value={link.knowledge_node_id} onChange={(event) => updateSource(sourceIndex, (current) => ({
                      ...current,
                      knowledge_links: current.knowledge_links.map((item, index) => index === linkIndex
                        ? { ...item, knowledge_node_id: Number(event.target.value) } : item),
                    }))}>
                      {editor.knowledge_options.map((option) => <option key={option.id} value={option.id}>{option.code ? `${option.code} · ` : ""}{option.title}</option>)}
                    </select>
                    <select value={link.relation_type} onChange={(event) => updateSource(sourceIndex, (current) => ({
                      ...current,
                      knowledge_links: current.knowledge_links.map((item, index) => index === linkIndex
                        ? { ...item, relation_type: event.target.value } : item),
                    }))}>
                      {KNOWLEDGE_RELATIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                    </select>
                    <button onClick={() => updateSource(sourceIndex, (current) => ({
                      ...current,
                      knowledge_links: current.knowledge_links.filter((_, index) => index !== linkIndex),
                    }))}>移除</button>
                  </div>
                ))}
                <button onClick={() => addKnowledge(sourceIndex)}>＋ 知识点</button>
              </div>
              <div className="subjective-link-list">
                {source.ability_links.map((link, linkIndex) => (
                  <div className="subjective-link-row ability" key={`ability-${linkIndex}`}>
                    <select value={link.ability_dimension_id} onChange={(event) => updateSource(sourceIndex, (current) => ({
                      ...current,
                      ability_links: current.ability_links.map((item, index) => index === linkIndex
                        ? { ...item, ability_dimension_id: Number(event.target.value) } : item),
                    }))}>
                      {editor.ability_options.map((option) => <option key={option.id} value={option.id}>{option.title}</option>)}
                    </select>
                    <select value={link.response_mode} onChange={(event) => updateSource(sourceIndex, (current) => ({
                      ...current,
                      ability_links: current.ability_links.map((item, index) => index === linkIndex
                        ? { ...item, response_mode: event.target.value } : item),
                    }))}>
                      {ABILITY_RESPONSE_MODES.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                    </select>
                    <label>证据强度 <input type="number" min="0" max="1" step="0.1" value={link.evidence_strength}
                      onChange={(event) => updateSource(sourceIndex, (current) => ({
                        ...current,
                        ability_links: current.ability_links.map((item, index) => index === linkIndex
                          ? { ...item, evidence_strength: Number(event.target.value) } : item),
                      }))} /></label>
                    <button onClick={() => updateSource(sourceIndex, (current) => ({
                      ...current,
                      ability_links: current.ability_links.filter((_, index) => index !== linkIndex),
                    }))}>移除</button>
                  </div>
                ))}
                <button onClick={() => addAbility(sourceIndex)}>＋ 能力维度</button>
              </div>
            </article>
          );
        })}
      </div>
      <div className="objective-actions">
        <button className="primary" disabled={saving} onClick={() => void save()}>
          {saving ? "正在另存…" : "确认链接，另存未来版本"}
        </button>
        <span className="meta">允许显式留空；留空表示本题暂不产生对应图谱证据。</span>
      </div>
    </details>
  );
}
