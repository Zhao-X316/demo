import { useMemo, useState } from "react";
import { kpCreate } from "../../api/exam";
import type { KnowledgePoint } from "../../api/exam";

export function KnowledgeTab({
  knowledge,
  onDone,
  onError,
}: {
  knowledge: KnowledgePoint[];
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [parentId, setParentId] = useState(0);
  const [busy, setBusy] = useState(false);
  const roots = useMemo(() => knowledge.filter((kp) => kp.parent_id == null), [knowledge]);

  const save = async () => {
    if (!name.trim()) {
      onError("知识点名称不能为空");
      return;
    }
    setBusy(true);
    try {
      await kpCreate(name.trim(), code.trim() || null, parentId || null);
      setName("");
      setCode("");
      onDone("知识点已创建");
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="exam-grid">
      <section className="exam-card">
        <div className="exam-card-head"><b>新增知识点</b></div>
        <div className="form exam-form">
          <label className="field"><span className="fl">名称</span><input type="text" value={name} onChange={(event) => setName(event.target.value)} placeholder="如 动量守恒" /></label>
          <label className="field"><span className="fl">编码（可选）</span><input type="text" value={code} onChange={(event) => setCode(event.target.value)} placeholder="如 PHY-MOM" /></label>
          <label className="field"><span className="fl">上级板块（可选）</span><select value={parentId} onChange={(event) => setParentId(Number(event.target.value))}><option value={0}>根节点</option>{roots.map((kp) => <option key={kp.id} value={kp.id}>{kp.name}</option>)}</select></label>
          <button className="primary" disabled={busy} onClick={save}>{busy ? "保存中…" : "创建知识点"}</button>
        </div>
      </section>
      <section className="exam-card">
        <div className="exam-card-head"><b>知识点树</b><span className="tag">{knowledge.length} 个</span></div>
        {roots.length === 0 && <div className="empty-state">还没有知识点。</div>}
        {roots.map((root) => (
          <div className="kp-group" key={root.id}>
            <div className="kp-root"><span>◆</span><b>{root.name}</b>{root.code && <code>{root.code}</code>}</div>
            {knowledge.filter((kp) => kp.parent_id === root.id).map((child) => <div className="kp-child" key={child.id}><span>└</span>{child.name}{child.code && <code>{child.code}</code>}</div>)}
          </div>
        ))}
      </section>
    </div>
  );
}
