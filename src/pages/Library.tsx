import { useEffect, useMemo, useState } from "react";
import {
  Class,
  RecContent,
  Student,
  TaskGenerateResult,
  classesList,
  contentsDelete,
  contentsList,
  contentsSetEnabled,
  contentsUpsert,
  studentsList,
} from "../api/manage";
import { Modal } from "../components/ui";
import { AssignModal, SyllabusModal } from "../components/modals";

export default function Library() {
  const [contents, setContents] = useState<RecContent[]>([]);
  const [students, setStudents] = useState<Student[]>([]);
  const [classes, setClasses] = useState<Class[]>([]);
  const [open, setOpen] = useState<Record<number, boolean>>({});
  const [picked, setPicked] = useState<number[]>([]);
  const [q, setQ] = useState("");
  const [modal, setModal] = useState<"syllabus" | "assign" | "settings" | null>(null);
  const [toast, setToast] = useState("");
  const [err, setErr] = useState("");

  const taskResultText = (r: TaskGenerateResult) => {
    const parts = [`新建 ${r.created}`];
    if (r.revived) parts.push(`恢复 ${r.revived}`);
    if (r.skipped_open) parts.push(`跳过未完成 ${r.skipped_open}`);
    if (r.skipped_completed) parts.push(`跳过已完成 ${r.skipped_completed}`);
    return parts.join("，");
  };

  const load = () => contentsList().then(setContents).catch((e) => setErr(String(e)));
  useEffect(() => {
    load();
    studentsList().then(setStudents).catch(() => undefined);
    classesList().then(setClasses).catch(() => undefined);
  }, []);

  const filtered = useMemo(
    () => contents.filter((c) => !q || c.content_no.includes(q) || c.title.includes(q)),
    [contents, q],
  );
  // 按「课」前缀分组（content_no 形如 道法8上-04课-05 → 道法8上-04课）
  const groups = useMemo(() => {
    const m = new Map<string, RecContent[]>();
    for (const c of filtered) {
      const mt = c.content_no.match(/^(.*课)/);
      const k = mt ? mt[1] : "其他";
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(c);
    }
    return [...m.entries()];
  }, [filtered]);

  const togglePick = (id: number) =>
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));

  return (
    <div className="page">
      <div className="page-head">
        <h1>内容库</h1>
        <span className="date">共 {contents.length} 篇 · 全校共享</span>
      </div>
      <div className="sub">
        背诵内容仓库（按教材组织）。勾选若干篇即可「布置给某班」。新建 / 修改 / 删除内容在「内容设置」里（这里只浏览，防误触）。
      </div>
      {err && <div className="error">{err}</div>}
      {toast && <div className="ok-banner">{toast}</div>}
      <div className="toolbar">
        <input className="search" placeholder="搜索 编号 / 标题…" value={q} onChange={(e) => setQ(e.target.value)} />
        <div className="spacer" />
        {picked.length > 0 && (
          <button className="primary" onClick={() => setModal("assign")}>
            布置选中 {picked.length} 篇 →
          </button>
        )}
        <button onClick={() => setModal("syllabus")}>📄 导入背诵清单</button>
        <button onClick={() => setModal("settings")}>⚙ 内容设置</button>
      </div>

      {contents.length === 0 && <div className="loading">还没有背诵内容，点「导入背诵清单」批量建立，或在「内容设置」里手动新建</div>}

      {groups.map(([lesson, items]) => (
        <div className="lesson" key={lesson}>
          <div className="lhead">
            <span className="lt">{lesson}</span>
            <span className="lc">{items.length} 篇</span>
          </div>
          {items.map((c) => {
            const star = c.title.startsWith("★");
            return (
              <div className="citem" key={c.id} onClick={() => setOpen((o) => ({ ...o, [c.id]: !o[c.id] }))}>
                <div className="crow">
                  <input
                    type="checkbox"
                    checked={picked.includes(c.id)}
                    onClick={(e) => e.stopPropagation()}
                    onChange={() => togglePick(c.id)}
                  />
                  <span className="cno">{c.content_no}</span>
                  <span className="ctitle">
                    {star ? c.title.replace(/^★\s*/, "") : c.title}{" "}
                    {star && <span className="star">★ 重点</span>}
                    {!c.enabled && <span className="tag" style={{ marginLeft: 6 }}>已停用</span>}
                  </span>
                  <span className="muted" style={{ fontSize: 12 }}>{open[c.id] ? "收起" : "展开"}</span>
                </div>
                {open[c.id] && <div className="cans">{c.answer_text}</div>}
              </div>
            );
          })}
        </div>
      ))}

      {modal === "syllabus" && (
        <SyllabusModal
          onClose={() => setModal(null)}
          onImported={(n) => {
            setModal(null);
            setToast(`已导入 ${n} 篇背诵内容`);
            load();
          }}
        />
      )}
      {modal === "assign" && (
        <AssignModal
          students={students}
          contents={contents}
          classList={classes}
          presetContentIds={picked}
          onClose={() => setModal(null)}
          onDone={(result) => {
            setModal(null);
            setPicked([]);
            setToast(`已布置：${taskResultText(result)}`);
          }}
        />
      )}
      {modal === "settings" && (
        <ContentSettingsModal
          onClose={() => {
            setModal(null);
            load();
          }}
        />
      )}
    </div>
  );
}

/* ───────── 内容设置：新建 / 修改 / 停用 / 删除 ───────── */
function ContentSettingsModal({ onClose }: { onClose: () => void }) {
  const [list, setList] = useState<RecContent[]>([]);
  const [q, setQ] = useState("");
  const [subjF, setSubjF] = useState<string>("all");
  const [volF, setVolF] = useState<string>("all");
  const [picked, setPicked] = useState<number[]>([]);
  const [editId, setEditId] = useState<number | null>(null);
  const [eTitle, setETitle] = useState("");
  const [eAns, setEAns] = useState("");
  // 新建
  const [nNo, setNNo] = useState("");
  const [nTitle, setNTitle] = useState("");
  const [nAns, setNAns] = useState("");
  const [showNew, setShowNew] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [err, setErr] = useState("");

  const refresh = () => contentsList().then(setList).catch((e) => setErr(String(e)));
  useEffect(() => {
    refresh();
  }, []);

  // 解析 content_no（道法8上-04课-05）→ 科目/年级册，用于筛选
  const parseNo = (no: string) => {
    const m = no.split("-")[0]?.match(/^(\D+?)(\d+[上下])$/);
    return { subject: m ? m[1] : "其他", vol: m ? m[2] : "—" };
  };
  const uniq = (xs: string[]) => [...new Set(xs)].sort((a, b) => a.localeCompare(b, "zh"));
  const subjects = uniq(list.map((c) => parseNo(c.content_no).subject));
  // 年级册选项随科目收窄
  const vols = uniq(
    list.filter((c) => subjF === "all" || parseNo(c.content_no).subject === subjF).map((c) => parseNo(c.content_no).vol),
  );
  const view = list.filter((c) => {
    const { subject, vol } = parseNo(c.content_no);
    return (
      (!q || c.content_no.includes(q) || c.title.includes(q)) &&
      (subjF === "all" || subject === subjF) &&
      (volF === "all" || vol === volF)
    );
  });
  const togglePick = (id: number) =>
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));
  const batchEnabled = async (enabled: boolean) => {
    await contentsSetEnabled(picked, enabled);
    setNote(`已${enabled ? "启用" : "停用"} ${picked.length} 篇`);
    setPicked([]);
    refresh();
  };
  const batchDelete = async () => {
    if (!window.confirm(`确认删除选中的 ${picked.length} 篇内容？已有任务、提交或复习记录的内容不会被删除，建议改为停用。`)) {
      return;
    }
    const r = await contentsDelete(picked);
    setNote(`删除 ${r.deleted} 篇` + (r.blocked.length ? `；${r.blocked.length} 篇有历史记录，已阻止删除（建议停用）` : ""));
    setPicked([]);
    refresh();
  };

  const create = async () => {
    setBusy(true);
    setErr("");
    try {
      await contentsUpsert(nNo, nTitle, nAns, true);
      setNNo("");
      setNTitle("");
      setNAns("");
      setShowNew(false);
      setNote("已新建背诵内容");
      refresh();
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };
  const startEdit = (c: RecContent) => {
    setEditId(c.id);
    setETitle(c.title);
    setEAns(c.answer_text);
  };
  const saveEdit = async (c: RecContent) => {
    await contentsUpsert(c.content_no, eTitle, eAns, c.enabled);
    setEditId(null);
    setNote("已修改");
    refresh();
  };

  return (
    <Modal
      title="内容设置"
      onClose={onClose}
      footer={<><span className="sub" style={{ margin: 0 }}>共 {list.length} 篇</span><button className="primary" onClick={onClose}>完成</button></>}
    >
      {err && <div className="error">{err}</div>}
      {note && <div className="ok-banner" style={{ margin: "0 0 12px" }}>{note}</div>}

      <div className="frow" style={{ marginBottom: 8 }}>
        <input className="search" style={{ flex: 1, maxWidth: "100%" }} placeholder="搜索 编号 / 标题…" value={q} onChange={(e) => setQ(e.target.value)} />
        <button className="primary" onClick={() => setShowNew((v) => !v)}>{showNew ? "收起" : "＋ 新建"}</button>
      </div>

      {(subjects.length > 1 || vols.length > 1) && (
        <div style={{ marginBottom: 6 }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 4 }}>
            <span className="muted" style={{ fontSize: 12, width: 40, flexShrink: 0 }}>科目</span>
            <div>
              <span className={"chip" + (subjF === "all" ? " on" : "")} onClick={() => { setSubjF("all"); setVolF("all"); }}>全部</span>
              {subjects.map((s) => (
                <span key={s} className={"chip" + (subjF === s ? " on" : "")} onClick={() => { setSubjF(s); setVolF("all"); }}>{s}</span>
              ))}
            </div>
          </div>
          <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
            <span className="muted" style={{ fontSize: 12, width: 40, flexShrink: 0 }}>年级</span>
            <div>
              <span className={"chip" + (volF === "all" ? " on" : "")} onClick={() => setVolF("all")}>全部</span>
              {vols.map((v) => (
                <span key={v} className={"chip" + (volF === v ? " on" : "")} onClick={() => setVolF(v)}>{v}</span>
              ))}
            </div>
          </div>
        </div>
      )}

      {picked.length > 0 && (
        <div className="row" style={{ background: "var(--brand-weak)", borderColor: "var(--brand-line)" }}>
          <b style={{ color: "var(--brand)", fontSize: 13 }}>已选 {picked.length} 篇</b>
          <div className="spacer" />
          {picked.length === 1 && (
            <button
              className="sm"
              onClick={() => {
                const c = list.find((x) => x.id === picked[0]);
                if (c) startEdit(c);
              }}
            >
              修改
            </button>
          )}
          <button className="sm" onClick={() => batchEnabled(false)}>停用</button>
          <button className="sm" onClick={() => batchEnabled(true)}>启用</button>
          <button className="sm danger" onClick={batchDelete}>删除</button>
          <button className="sm" onClick={() => setPicked([])}>取消</button>
        </div>
      )}

      {showNew && (
        <div style={{ border: "1px solid var(--brand)", borderRadius: 9, padding: 12, marginBottom: 14 }}>
          <div className="form" style={{ maxWidth: "100%" }}>
            <div className="field">
              <div className="fl">编号</div>
              <input type="text" value={nNo} onChange={(e) => setNNo(e.target.value)} placeholder="道法8上-04课-05" />
            </div>
            <div className="field">
              <div className="fl">标题</div>
              <input type="text" value={nTitle} onChange={(e) => setNTitle(e.target.value)} placeholder="第四课·社会生活讲道德 ｜ 为什么要以礼待人？" />
            </div>
            <div className="field">
              <div className="fl">答案原文</div>
              <textarea rows={4} value={nAns} onChange={(e) => setNAns(e.target.value)} />
            </div>
            <div>
              <button className="primary" disabled={busy || !nNo || !nTitle} onClick={create}>保存新建</button>
            </div>
          </div>
        </div>
      )}

      {view.length === 0 && <div className="muted" style={{ fontSize: 13 }}>没有内容</div>}
      {view.map((c) =>
        editId === c.id ? (
          <div key={c.id} style={{ border: "1px solid var(--brand)", borderRadius: 9, padding: 12, marginBottom: 6 }}>
            <div className="meta" style={{ marginBottom: 6 }}>{c.content_no}（编号不可改）</div>
            <div className="field" style={{ marginBottom: 8 }}>
              <div className="fl">标题</div>
              <input type="text" value={eTitle} onChange={(e) => setETitle(e.target.value)} />
            </div>
            <div className="field" style={{ marginBottom: 8 }}>
              <div className="fl">答案原文</div>
              <textarea rows={4} value={eAns} onChange={(e) => setEAns(e.target.value)} />
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button className="primary sm" disabled={!eTitle} onClick={() => saveEdit(c)}>保存</button>
              <button className="sm" onClick={() => setEditId(null)}>取消</button>
            </div>
          </div>
        ) : (
          <label className="row" key={c.id} style={{ cursor: "pointer" }}>
            <input type="checkbox" checked={picked.includes(c.id)} onChange={() => togglePick(c.id)} />
            <span className="cno">{c.content_no}</span>
            <div style={{ minWidth: 0 }}>
              <div className="who" style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 360 }}>
                {c.title.replace(/^★\s*/, "")}
                {!c.enabled && <span className="tag" style={{ marginLeft: 6 }}>已停用</span>}
              </div>
            </div>
          </label>
        ),
      )}
    </Modal>
  );
}
