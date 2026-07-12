import { CSSProperties, useEffect, useMemo, useState } from "react";
import { Modal } from "./ui";
import { Class, RecContent, Student, TaskGenerateResult, contentsImport, parseSyllabus, tasksGenerate, tasksReassign, tasksRemove } from "../api/manage";
import { ParsedContent } from "../api/manage";
import { TaskCard, dashboardToday } from "../api/dashboard";

/* ───────── 布置背诵 ───────── */
// 内容共享、布置分班：选学生（可按班级全选）× 选内容 → tasks_generate(pairs)。
export function AssignModal({
  students,
  contents,
  classList,
  presetContentIds,
  onClose,
  onDone,
}: {
  students: Student[];
  contents: RecContent[];
  classList?: Class[];
  presetContentIds?: number[];
  onClose: () => void;
    onDone: (result: TaskGenerateResult) => void;
}) {
  const enabledStudents = students.filter((s) => s.enabled);
  const clsName = (id: number | null) =>
    id == null ? "未分班" : classList?.find((c) => c.id === id)?.name ?? `班级 ${id}`;
  const groupedClasses = useMemo(() => {
    const m = new Map<string, Student[]>();
    for (const s of enabledStudents) {
      const k = clsName(s.class_id);
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(s);
    }
    return [...m.entries()];
  }, [students, classList]);
  const [pickStu, setPickStu] = useState<number[]>(enabledStudents.map((s) => s.id));
  const [pickCon, setPickCon] = useState<number[]>(presetContentIds ?? []);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [tab, setTab] = useState<"assign" | "manage">("assign");
  const [manageClassId, setManageClassId] = useState<number | "un" | null>(classList?.[0]?.id ?? null);
  const [assigned, setAssigned] = useState<TaskCard[]>([]); // 选中班级当前已布置（参考）

  const toggleStu = (id: number) =>
    setPickStu((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));
  const toggleCon = (id: number) =>
    setPickCon((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));
  const toggleClass = (list: Student[]) => {
    const ids = list.map((s) => s.id);
    const allOn = ids.every((id) => pickStu.includes(id));
    setPickStu((p) => (allOn ? p.filter((id) => !ids.includes(id)) : [...new Set([...p, ...ids])]));
  };

  // ② 阶梯选择：学科 → 年级册 → 课 → 知识点。content_no 形如 道法8上-04课-05
  const parseNo = (no: string) => {
    const parts = no.split("-");
    const m = (parts[0] ?? "").match(/^(\D+?)(\d+[上下].*)$/); // 道法 / 8上
    return { subject: m ? m[1] : "其他", vol: m ? m[2] : "—", lesson: parts[1] ?? "—" };
  };
  const uniq = (xs: string[]) => [...new Set(xs)].sort((a, b) => a.localeCompare(b, "zh"));
  const parsed = useMemo(
    () => contents.filter((c) => c.enabled).map((c) => ({ c, ...parseNo(c.content_no) })),
    [contents],
  );
  const subjects = uniq(parsed.map((p) => p.subject));
  // 默认落点：① 从内容库布置带来的 preset 内容所属教材；② 否则所选学生占多数的班级绑定的教材（班级对应）
  const presetParse = presetContentIds?.length
    ? parsed.find((p) => presetContentIds.includes(p.c.id))
    : undefined;
  const classTextbook = useMemo(() => {
    if (tab === "manage") {
      return typeof manageClassId === "number" ? classList?.find((c) => c.id === manageClassId)?.textbook ?? null : null;
    }
    const cnt = new Map<number, number>();
    for (const sid of pickStu) {
      const st = students.find((s) => s.id === sid);
      if (st?.class_id != null) cnt.set(st.class_id, (cnt.get(st.class_id) ?? 0) + 1);
    }
    let best: number | null = null;
    let bn = 0;
    for (const [cid, n] of cnt) if (n > bn) { bn = n; best = cid; }
    return best != null ? classList?.find((c) => c.id === best)?.textbook ?? null : null;
  }, [tab, manageClassId, pickStu, students, classList]);
  const def: { subject: string; vol: string; lesson?: string } | undefined = presetParse
    ? { subject: presetParse.subject, vol: presetParse.vol, lesson: presetParse.lesson }
    : (() => {
        const tb = classTextbook?.split(",")[0]?.trim(); // 班级可能绑多册，默认落第一册
        const m = tb?.match(/^(\D+?)(\d+[上下])$/);
        return m ? { subject: m[1], vol: m[2] } : undefined;
      })();

  const [subjRaw, setSubj] = useState<string | null>(null);
  const subj = subjRaw && subjects.includes(subjRaw)
    ? subjRaw
    : def && subjects.includes(def.subject) ? def.subject : subjects[0] ?? "";
  const vols = uniq(parsed.filter((p) => p.subject === subj).map((p) => p.vol));
  const [volRaw, setVol] = useState<string | null>(null);
  const vol = volRaw && vols.includes(volRaw)
    ? volRaw
    : def && subj === def.subject && def.vol && vols.includes(def.vol) ? def.vol : vols[0] ?? "";
  const lessons = uniq(parsed.filter((p) => p.subject === subj && p.vol === vol).map((p) => p.lesson));
  const [lessonRaw, setLesson] = useState<string | null>(null);
  const lesson = lessonRaw && lessons.includes(lessonRaw)
    ? lessonRaw
    : def && subj === def.subject && vol === def.vol && def.lesson && lessons.includes(def.lesson) ? def.lesson : lessons[0] ?? "";
  const points = parsed.filter((p) => p.subject === subj && p.vol === vol && p.lesson === lesson);

  const pairCount = pickStu.length * pickCon.length;
  const submit = async () => {
    setBusy(true);
    setErr("");
    try {
      const pairs: [number, number][] = [];
      for (const s of pickStu) for (const c of pickCon) pairs.push([s, c]);
      const result = await tasksGenerate(pairs);
      onDone(result);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  // ── 修改已布置：选班级 → 选新内容 → 覆盖该班今日「新背」 ──
  // 选中分组的学生（具体班级 / 未分班）
  const groupStudents =
    manageClassId === "un"
      ? enabledStudents.filter((s) => s.class_id == null)
      : typeof manageClassId === "number"
        ? enabledStudents.filter((s) => s.class_id === manageClassId)
        : [];
  const manageStudents = groupStudents.map((s) => s.id);
  const manageLabel = manageClassId === "un" ? "未分班" : typeof manageClassId === "number" ? clsName(manageClassId) : "—";
  // 拉该分组当前已布置的「新背」作参考（按学号匹配）
  const [delCon, setDelCon] = useState<string[]>([]);
  const loadAssigned = () => {
    if (manageClassId == null) {
      setAssigned([]);
      return;
    }
    const nos = new Set(groupStudents.map((s) => s.student_no));
    dashboardToday()
      .then((v) => setAssigned(v.normal.filter((t) => nos.has(t.student_no))))
      .catch(() => setAssigned([]));
  };
  useEffect(() => {
    if (tab !== "manage") {
      setAssigned([]);
      return;
    }
    setDelCon([]);
    loadAssigned();
  }, [tab, manageClassId, students]);
  // 当前已布置的去重内容（展示 + 可多选删除）
  const currentContents = [...new Map(assigned.map((t) => [t.content_no, t.content_title])).entries()];
  const toggleDel = (no: string) => setDelCon((p) => (p.includes(no) ? p.filter((x) => x !== no) : [...p, no]));
  const removeSelected = async () => {
    setBusy(true);
    setErr("");
    try {
      const ids = delCon
        .map((no) => contents.find((c) => c.content_no === no)?.id)
        .filter((x): x is number => x != null);
      await tasksRemove(manageStudents, ids);
      setDelCon([]);
      loadAssigned();
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };
  const overwrite = async () => {
    setBusy(true);
      setErr("");
      try {
        const n = await tasksReassign(manageStudents, pickCon);
        onDone({ created: n, revived: 0, skipped_open: 0, skipped_completed: 0, total_effective: n });
      } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  const footer =
    tab === "assign" ? (
      <>
        <span className="sub" style={{ margin: 0 }}>
          给 <b style={{ color: "var(--brand)" }}>{pickStu.length}</b> 名学生布置{" "}
          <b style={{ color: "var(--brand)" }}>{pickCon.length}</b> 篇 · 今日到期 · {pairCount} 条任务
        </span>
        <div style={{ display: "flex", gap: 10 }}>
          <button onClick={onClose}>取消</button>
          <button className="primary" disabled={busy || pairCount === 0} onClick={submit}>
            {busy ? "布置中…" : "布置"}
          </button>
        </div>
      </>
    ) : (
      <>
        <span className="sub" style={{ margin: 0 }}>
          把 <b style={{ color: "var(--brand)" }}>{manageLabel}</b>{" "}
          未开始的已布置覆盖为 <b style={{ color: "var(--brand)" }}>{pickCon.length}</b> 篇（已开始的不动）
        </span>
        <div style={{ display: "flex", gap: 10 }}>
          <button onClick={onClose}>取消</button>
          <button
            className="primary"
            disabled={busy || manageClassId == null || pickCon.length === 0 || manageStudents.length === 0}
            onClick={overwrite}
          >
            {busy ? "覆盖中…" : "完成（覆盖）"}
          </button>
        </div>
      </>
    );

  return (
    <Modal title="布置背诵" onClose={onClose} footer={footer}>
      <div style={{ display: "flex", gap: 6, marginBottom: 14, borderBottom: "1px solid var(--line)" }}>
        <button
          className={tab === "assign" ? "tab on" : "tab"}
          style={tabStyle(tab === "assign")}
          onClick={() => setTab("assign")}
        >
          布置新任务
        </button>
        <button
          className={tab === "manage" ? "tab on" : "tab"}
          style={tabStyle(tab === "manage")}
          onClick={() => setTab("manage")}
        >
          修改已布置
        </button>
      </div>
      {err && <div className="error">{err}</div>}

      {tab === "assign" && (
        <div className="field" style={{ marginBottom: 16 }}>
          <div className="fl">① 布置给谁（点班级名可整班全选 / 取消）</div>
          {groupedClasses.map(([cls, list]) => (
            <div key={cls} style={{ marginBottom: 8 }}>
              <span className="chip" onClick={() => toggleClass(list)}>
                {cls} · {list.length} 人
              </span>
              {list.map((s) => (
                <span
                  key={s.id}
                  className={"chip" + (pickStu.includes(s.id) ? " on" : "")}
                  onClick={() => toggleStu(s.id)}
                >
                  {s.name}
                </span>
              ))}
            </div>
          ))}
        </div>
      )}

      {tab === "manage" && (
        <div className="field" style={{ marginBottom: 16 }}>
          <div className="fl">① 改哪个班的已布置</div>
          {(classList ?? []).length === 0 && <div className="muted" style={{ fontSize: 13 }}>还没有班级</div>}
          {(classList ?? []).map((c) => (
            <span
              key={c.id}
              className={"chip" + (manageClassId === c.id ? " on" : "")}
              onClick={() => setManageClassId(c.id)}
            >
              {c.name}
            </span>
          ))}
          <span
            className={"chip" + (manageClassId === "un" ? " on" : "")}
            onClick={() => setManageClassId("un")}
          >
            未分班
          </span>
          <div style={{ marginTop: 10 }}>
            <div className="fl" style={{ marginBottom: 6 }}>该班当前已布置（点选可删除，含已开始的）</div>
            {currentContents.length === 0 && <span className="muted" style={{ fontSize: 13 }}>无</span>}
            {currentContents.map(([no, title]) => (
              <span key={no} className={"chip" + (delCon.includes(no) ? " on" : "")} onClick={() => toggleDel(no)}>
                {delCon.includes(no) ? "✓ " : ""}
                {title || no}
              </span>
            ))}
            {delCon.length > 0 && (
              <div style={{ marginTop: 6 }}>
                <button className="sm danger" disabled={busy} onClick={removeSelected}>
                  删除选中 {delCon.length} 项内容
                </button>
              </div>
            )}
          </div>
          <div className="hint" style={{ marginTop: 10 }}>
            <b>删除</b>：上面点选内容 → 删除，会把该班这些内容今日的任务全部撤掉（已开始的也撤，提交/判定记录保留）。<br />
            <b>覆盖</b>：下面选新内容 → 「完成（覆盖）」，把<b>未开始</b>的换成新内容，已开始的不动。
          </div>
        </div>
      )}

      <div className="field">
        <div className="fl">
          {tab === "manage" ? "② 换成哪几篇" : "② 背哪几篇"}（学科 → 年级册 → 课 → 勾知识点；默认当前教材，可改）
        </div>
        <CascadeRow label="学科" opts={subjects} cur={subj} onPick={(v) => { setSubj(v); setVol(null); setLesson(null); }} />
        <CascadeRow label="年级册" opts={vols} cur={vol} onPick={(v) => { setVol(v); setLesson(null); }} />
        <CascadeRow label="课" opts={lessons} cur={lesson} onPick={(v) => setLesson(v)} />
        <div style={{ marginTop: 8 }}>
          {points.length === 0 && <span className="muted" style={{ fontSize: 13 }}>该课暂无知识点</span>}
          {points.map(({ c }) => {
            const star = c.title.startsWith("★");
            const label = (c.title.includes("｜") ? c.title.split("｜").pop()!.trim() : c.title).replace(/^★\s*/, "");
            return (
              <span
                key={c.id}
                className={"chip" + (pickCon.includes(c.id) ? " on" : "")}
                onClick={() => toggleCon(c.id)}
              >
                {pickCon.includes(c.id) ? "✓ " : ""}
                {star ? "★ " : ""}
                {label}
              </span>
            );
          })}
        </div>
        {pickCon.length > 0 && (
          <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
            已选 {pickCon.length} 篇（可跨课累加）·{" "}
            <button className="link" onClick={() => setPickCon([])}>清空</button>
          </div>
        )}
      </div>
    </Modal>
  );
}

function tabStyle(on: boolean): CSSProperties {
  return {
    border: "none",
    background: "none",
    padding: "8px 4px",
    marginBottom: -1,
    fontSize: 13.5,
    fontWeight: 600,
    color: on ? "var(--brand)" : "var(--muted)",
    borderBottom: on ? "2px solid var(--brand)" : "2px solid transparent",
    borderRadius: 0,
    cursor: "pointer",
  };
}

function CascadeRow({
  label,
  opts,
  cur,
  onPick,
}: {
  label: string;
  opts: string[];
  cur: string;
  onPick: (v: string) => void;
}) {
  if (!opts.length) return null;
  return (
    <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 6 }}>
      <span className="muted" style={{ fontSize: 12, width: 48, flexShrink: 0 }}>{label}</span>
      <div>
        {opts.map((o) => (
          <span key={o} className={"chip" + (cur === o ? " on" : "")} onClick={() => onPick(o)}>
            {o}
          </span>
        ))}
      </div>
    </div>
  );
}

/* ───────── 导入背诵清单 · 解析预览树 ───────── */
type TreeNode = ParsedContent & { _del?: boolean };
export function SyllabusModal({ onClose, onImported }: { onClose: () => void; onImported: (n: number) => void }) {
  const [text, setText] = useState("");
  const [prefix, setPrefix] = useState("道法8上");
  const [rows, setRows] = useState<TreeNode[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const doParse = async () => {
    setBusy(true);
    setErr("");
    try {
      const r = await parseSyllabus(text, prefix);
      setRows(r.map((x) => ({ ...x })));
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };
  const alive = (rows ?? []).filter((r) => !r._del);
  const groups = useMemo(() => {
    const m = new Map<string, TreeNode[]>();
    for (const r of rows ?? []) {
      if (!m.has(r.lesson_title)) m.set(r.lesson_title, []);
      m.get(r.lesson_title)!.push(r);
    }
    return [...m.entries()];
  }, [rows]);
  const setTitle = (no: string, v: string) =>
    setRows((rs) => rs!.map((r) => (r.content_no === no ? { ...r, title: v } : r)));
  const toggleKey = (no: string) =>
    setRows((rs) => rs!.map((r) => (r.content_no === no ? { ...r, is_key: !r.is_key } : r)));
  const del = (no: string) =>
    setRows((rs) => rs!.map((r) => (r.content_no === no ? { ...r, _del: !r._del } : r)));

  const doImport = async () => {
    setBusy(true);
    setErr("");
    try {
      const payload = alive.map((r) => ({
        content_no: r.content_no,
        title: (r.is_key ? "★ " : "") + r.title,
        answer_text: r.answer_text,
      }));
      const res = await contentsImport(payload);
      onImported(res.ok);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      title="导入背诵清单 · 解析预览"
      onClose={onClose}
      footer={
        rows ? (
          <>
            <span className="sub" style={{ margin: 0 }}>
              解析出 <b style={{ color: "var(--brand)" }}>{alive.length}</b> 个知识点（按点切）· 编号按出现顺序重排
            </span>
            <div style={{ display: "flex", gap: 10 }}>
              <button onClick={() => setRows(null)}>重新粘贴</button>
              <button className="primary" disabled={busy || !alive.length} onClick={doImport}>
                {busy ? "导入中…" : `确认导入 ${alive.length} 篇`}
              </button>
            </div>
          </>
        ) : (
          <>
            <span className="sub" style={{ margin: 0 }}>粘贴背诵清单全文，自动按「第X课 / 1. 2. 3.」切成一条条背诵内容</span>
            <button className="primary" disabled={busy || !text.trim()} onClick={doParse}>
              {busy ? "解析中…" : "解析"}
            </button>
          </>
        )
      }
    >
      {err && <div className="error">{err}</div>}
      {!rows ? (
        <div className="form" style={{ maxWidth: "100%" }}>
          <div className="field">
            <div className="fl">编号前缀（学科+年级册，如 道法8上）</div>
            <input type="text" value={prefix} onChange={(e) => setPrefix(e.target.value)} style={{ maxWidth: 200 }} />
          </div>
          <div className="field">
            <div className="fl">粘贴清单全文（.md / 复制的文本）</div>
            <textarea
              rows={12}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="第一课 丰富的社会生活1．……2．……第二课 网络生活新空间……"
            />
          </div>
        </div>
      ) : (
        <>
          <div className="hint" style={{ marginBottom: 16 }}>
            每个知识点 = 一篇背诵内容，标题取设问、带课题前缀。可改名 / 删 / 标 ★ 重点，确认后批量入库。
          </div>
          {groups.map(([lesson, pts]) => (
            <div className="tree-l" key={lesson}>
              <div className="tree-lh">
                <span style={{ color: "var(--brand)" }}>▸</span> {lesson}{" "}
                <span style={{ color: "var(--faint)", fontWeight: 400 }}>· {pts.filter((p) => !p._del).length} 点</span>
              </div>
              {pts.map((p) => (
                <div
                  className="tree-p"
                  key={p.content_no}
                  style={p._del ? { opacity: 0.4, textDecoration: "line-through" } : {}}
                >
                  <span className="pn">{p.content_no}</span>
                  <input value={p.title} onChange={(e) => setTitle(p.content_no, e.target.value)} />
                  <button className="ico" style={{ color: p.is_key ? "var(--warn)" : "var(--faint)" }} onClick={() => toggleKey(p.content_no)}>
                    ★
                  </button>
                  <button className="ico" onClick={() => del(p.content_no)}>
                    {p._del ? "↺" : "🗑"}
                  </button>
                </div>
              ))}
            </div>
          ))}
        </>
      )}
    </Modal>
  );
}
