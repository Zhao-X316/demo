import { useEffect, useMemo, useState } from "react";
import {
  Class,
  RecContent,
  Student,
  classCreate,
  classDelete,
  classUpdate,
  classesList,
  contentsList,
  studentsDelete,
  studentsImport,
  studentsList,
  studentsSetClass,
  studentsSetEnabled,
} from "../api/manage";
import { Avatar, Modal } from "../components/ui";

// 从内容库 content_no（道法8上-04课-05）抽出可绑教材选项「学科+年级册」（道法8上）
function textbookOptionsFrom(contents: RecContent[]): string[] {
  const set = new Set<string>();
  for (const c of contents) {
    const m = c.content_no.split("-")[0]?.match(/^(\D+?)(\d+[上下])$/);
    if (m) set.add(m[1] + m[2]);
  }
  return [...set].sort((a, b) => a.localeCompare(b, "zh"));
}

type ModalState =
  | { type: "import" | "classset" | "studentset" }
  | { type: "pick"; classId: number; className: string }
  | null;

export default function Students() {
  const [students, setStudents] = useState<Student[]>([]);
  const [classes, setClasses] = useState<Class[]>([]);
  const [tbOptions, setTbOptions] = useState<string[]>([]);
  const [modal, setModal] = useState<ModalState>(null);
  const [toast, setToast] = useState("");
  const [err, setErr] = useState("");

  const load = () => {
    studentsList().then(setStudents).catch((e) => setErr(String(e)));
    classesList().then(setClasses).catch(() => undefined);
    contentsList().then((c) => setTbOptions(textbookOptionsFrom(c))).catch(() => undefined);
  };
  useEffect(() => {
    load();
  }, []);

  const clsName = (id: number | null) =>
    id == null ? "未分班" : classes.find((c) => c.id === id)?.name ?? `班级 ${id}`;

  const groups = useMemo(() => {
    const out: { id: number | null; name: string; list: Student[] }[] = [];
    for (const c of classes) {
      out.push({ id: c.id, name: c.name, list: students.filter((s) => s.class_id === c.id) });
    }
    out.push({ id: null, name: "未分班", list: students.filter((s) => s.class_id == null) });
    return out;
  }, [students, classes]);

  return (
    <div className="page">
      <div className="page-head">
        <h1>学生</h1>
        <div style={{ display: "flex", gap: 10 }}>
          <button onClick={() => setModal({ type: "classset" })}>⚙ 班级设置</button>
          <button onClick={() => setModal({ type: "studentset" })}>⚙ 学生设置</button>
          <button className="primary" onClick={() => setModal({ type: "import" })}>导入班级 / 学生</button>
        </div>
      </div>
      <div className="sub">
        点班级后的「选择学生」把学生加进班。停用 / 删除学生在「学生设置」里操作（这里不放按钮，防误触）。
      </div>
      {err && <div className="error">{err}</div>}
      {toast && <div className="ok-banner">{toast}</div>}

      {students.length === 0 && classes.length === 0 && (
        <div className="loading">先「班级设置」建班，再「批量导入」学生（班级名一行、学生几行可一次建多班）</div>
      )}

      {groups.map((g) => (
        <div key={g.name}>
          <div className="sech">
            {g.name}
            <span className="n">{g.list.length} 人</span>
            {g.id != null && (
              <button
                className="link"
                style={{ marginLeft: 6 }}
                onClick={() => setModal({ type: "pick", classId: g.id!, className: g.name })}
              >
                选择学生
              </button>
            )}
          </div>
          {g.list.length === 0 && (
            <div className="muted" style={{ fontSize: 12, padding: "0 2px 8px" }}>
              {g.id != null ? "（空，点上方「选择学生」加入）" : "（无）"}
            </div>
          )}
          {g.list.map((s) => (
            <div className="row" key={s.id}>
              <Avatar name={s.name} />
              <div>
                <div className="who">
                  {s.name}
                  {!s.enabled && <span className="tag" style={{ marginLeft: 8 }}>已停用</span>}
                </div>
                <div className="meta">学号 {s.student_no}</div>
              </div>
            </div>
          ))}
        </div>
      ))}

      {modal?.type === "import" && (
        <ImportStudentsModal
          onClose={() => setModal(null)}
          onDone={(stu, cls) => {
            setModal(null);
            setToast(`已导入 ${stu} 名学生${cls ? `、${cls} 个班级` : ""}`);
            load();
          }}
        />
      )}
      {modal?.type === "classset" && (
        <ClassSettingsModal
          students={students}
          tbOptions={tbOptions}
          onClose={() => {
            setModal(null);
            load();
          }}
        />
      )}
      {modal?.type === "studentset" && (
        <StudentSettingsModal
          classes={classes}
          clsName={clsName}
          onClose={() => {
            setModal(null);
            load();
          }}
        />
      )}
      {modal?.type === "pick" && (
        <PickStudentsModal
          classId={modal.classId}
          className={modal.className}
          students={students}
          clsName={clsName}
          onClose={() => setModal(null)}
          onDone={(msg) => {
            setModal(null);
            setToast(msg);
            load();
          }}
        />
      )}
    </div>
  );
}

/* ───────── 学生设置：停用 / 启用 / 删除（含多选批量），统一在此防误触 ───────── */
function StudentSettingsModal({
  classes,
  clsName,
  onClose,
}: {
  classes: Class[];
  clsName: (id: number | null) => string;
  onClose: () => void;
}) {
  const [list, setList] = useState<Student[]>([]);
  const [picked, setPicked] = useState<number[]>([]);
  const [q, setQ] = useState("");
  const [clsFilter, setClsFilter] = useState<number | "all" | "un">("all");
  const [note, setNote] = useState("");
  const [err, setErr] = useState("");

  const refresh = () => studentsList().then(setList).catch((e) => setErr(String(e)));
  useEffect(() => {
    refresh();
  }, []);

  const matchCls = (s: Student) =>
    clsFilter === "all" ? true : clsFilter === "un" ? s.class_id == null : s.class_id === clsFilter;
  const view = list.filter((s) => matchCls(s) && (!q || s.name.includes(q) || s.student_no.includes(q)));
  const togglePick = (id: number) => setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));

  const setEnabled = async (ids: number[], enabled: boolean) => {
    await studentsSetEnabled(ids, enabled);
    setNote(`已${enabled ? "启用" : "停用"} ${ids.length} 名`);
    setPicked([]);
    refresh();
  };
  const del = async (ids: number[]) => {
    if (!window.confirm(`确认删除 ${ids.length} 名学生？已有历史记录的学生不会被删除，建议改为停用。`)) {
      return;
    }
    const r = await studentsDelete(ids);
    setNote(`删除 ${r.deleted} 名` + (r.blocked.length ? `；${r.blocked.length} 名有背诵记录删不掉（建议停用）` : ""));
    setPicked([]);
    refresh();
  };

  return (
    <Modal
      title="学生设置"
      onClose={onClose}
      footer={<><span className="sub" style={{ margin: 0 }}>共 {list.length} 名学生</span><button className="primary" onClick={onClose}>完成</button></>}
    >
      {err && <div className="error">{err}</div>}
      {note && <div className="ok-banner" style={{ margin: "0 0 12px" }}>{note}</div>}
      <input
        className="search"
        style={{ maxWidth: "100%", marginBottom: 10 }}
        placeholder="搜索 姓名 / 学号…"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      <div style={{ marginBottom: 10 }}>
        <span className={"chip" + (clsFilter === "all" ? " on" : "")} onClick={() => setClsFilter("all")}>全部</span>
        {classes.map((c) => (
          <span key={c.id} className={"chip" + (clsFilter === c.id ? " on" : "")} onClick={() => setClsFilter(c.id)}>
            {c.name}
          </span>
        ))}
        <span className={"chip" + (clsFilter === "un" ? " on" : "")} onClick={() => setClsFilter("un")}>未分班</span>
      </div>
      {picked.length > 0 && (
        <div className="row" style={{ background: "var(--brand-weak)", borderColor: "var(--brand-line)" }}>
          <b style={{ color: "var(--brand)", fontSize: 13 }}>已选 {picked.length} 人</b>
          <div className="spacer" />
          <button className="sm" onClick={() => setEnabled(picked, false)}>批量停用</button>
          <button className="sm" onClick={() => setEnabled(picked, true)}>批量启用</button>
          <button className="sm danger" onClick={() => del(picked)}>批量删除</button>
          <button className="sm" onClick={() => setPicked([])}>取消</button>
        </div>
      )}
      {view.length === 0 && <div className="muted" style={{ fontSize: 13 }}>没有学生</div>}
      {view.map((s) => (
        <div className="row" key={s.id}>
          <input type="checkbox" checked={picked.includes(s.id)} onChange={() => togglePick(s.id)} />
          <Avatar name={s.name} />
          <div>
            <div className="who">
              {s.name}
              {!s.enabled && <span className="tag" style={{ marginLeft: 8 }}>已停用</span>}
            </div>
            <div className="meta">学号 {s.student_no} · {clsName(s.class_id)}</div>
          </div>
          <div className="spacer" />
          <button className="sm" onClick={() => setEnabled([s.id], !s.enabled)}>{s.enabled ? "停用" : "启用"}</button>
          <button className="sm danger" onClick={() => del([s.id])}>删除</button>
        </div>
      ))}
    </Modal>
  );
}

/* ───────── 教材多选（选年级 = 绑上下两册） ───────── */
const gradeCn = (g: string) => (({ "7": "七", "8": "八", "9": "九" } as Record<string, string>)[g] ?? g);

function TextbookPicker({ options, value, onChange }: { options: string[]; value: string[]; onChange: (v: string[]) => void }) {
  if (!options.length) {
    return <div className="muted" style={{ fontSize: 12 }}>先在内容库导入背诵清单，这里才有教材可选</div>;
  }
  const groups = new Map<string, { subject: string; grade: string; vols: { vol: string; tb: string }[] }>();
  for (const o of options) {
    const m = o.match(/^(\D+?)(\d+)([上下])$/);
    if (!m) continue;
    const key = m[1] + m[2];
    if (!groups.has(key)) groups.set(key, { subject: m[1], grade: m[2], vols: [] });
    groups.get(key)!.vols.push({ vol: m[3], tb: o });
  }
  const toggle = (tb: string) => onChange(value.includes(tb) ? value.filter((x) => x !== tb) : [...value, tb]);
  const toggleGroup = (vols: { tb: string }[]) => {
    const tbs = vols.map((v) => v.tb);
    const allOn = tbs.every((t) => value.includes(t));
    onChange(allOn ? value.filter((x) => !tbs.includes(x)) : [...new Set([...value, ...tbs])]);
  };
  return (
    <div>
      {[...groups.values()].map((g) => (
        <div key={g.subject + g.grade} style={{ marginBottom: 5 }}>
          <span className="chip" onClick={() => toggleGroup(g.vols)}>
            {g.subject} {gradeCn(g.grade)}年级
          </span>
          {g.vols.map((v) => (
            <span key={v.tb} className={"chip" + (value.includes(v.tb) ? " on" : "")} onClick={() => toggle(v.tb)}>
              {v.vol}
            </span>
          ))}
        </div>
      ))}
    </div>
  );
}

/* ───────── 班级设置：新建 / 改名 / 教材多选 / 删除（可连学生） ───────── */
function ClassSettingsModal({
  students,
  tbOptions,
  onClose,
}: {
  students: Student[];
  tbOptions: string[];
  onClose: () => void;
}) {
  const [list, setList] = useState<Class[]>([]);
  const [editId, setEditId] = useState<number | null>(null);
  const [eName, setEName] = useState("");
  const [eTb, setETb] = useState<string[]>([]);
  const [eTerm, setETerm] = useState("");
  const [err, setErr] = useState("");
  const [note, setNote] = useState("");
  const [confirmDel, setConfirmDel] = useState<number | null>(null);

  const refresh = () => classesList().then(setList).catch((e) => setErr(String(e)));
  useEffect(() => {
    refresh();
  }, []);

  const inClass = (id: number) => students.filter((s) => s.class_id === id);
  const startEdit = (c: Class) => {
    setEditId(c.id);
    setEName(c.name);
    setETb(c.textbook ? c.textbook.split(",").filter(Boolean) : []);
    setETerm(c.term ?? "");
  };
  const saveEdit = async () => {
    if (editId == null) return;
    await classUpdate(editId, eName, eTb.join(",") || undefined, eTerm || undefined);
    setEditId(null);
    setNote("已更新班级");
    refresh();
  };
  const delClassOnly = async (id: number) => {
    await classDelete(id);
    setConfirmDel(null);
    setNote("已删除班级，学生移回「未分班」");
    refresh();
  };
  const delWithStudents = async (id: number) => {
    const ids = inClass(id).map((s) => s.id);
    let msg = "";
    if (ids.length) {
      const r = await studentsDelete(ids);
      msg = `删除 ${r.deleted} 名学生` + (r.blocked.length ? `（${r.blocked.length} 名有背诵记录删不掉，已留未分班）` : "");
    }
    await classDelete(id);
    setConfirmDel(null);
    setNote(`已删除班级。${msg}`);
    refresh();
  };

  return (
    <Modal
      title="班级设置"
      onClose={onClose}
      footer={<><span className="sub" style={{ margin: 0 }}>共 {list.length} 个班级</span><button className="primary" onClick={onClose}>完成</button></>}
    >
      {err && <div className="error">{err}</div>}
      {note && <div className="ok-banner" style={{ margin: "0 0 12px" }}>{note}</div>}
      <div className="hint" style={{ marginBottom: 14 }}>
        班级在「导入班级 / 学生」里按「班级名一行」自动建。这里可<b>改名</b>、绑<b>教材</b>（多选；点「八年级」一次绑八上+八下）、删除。
      </div>

      <div>
        {list.length === 0 && (
          <div className="muted" style={{ fontSize: 13 }}>还没有班级，去「导入班级 / 学生」按班级名导入即可自动建</div>
        )}
        {list.map((c) =>
          editId === c.id ? (
            <div key={c.id} style={{ border: "1px solid var(--brand)", borderRadius: 9, padding: 12, marginBottom: 6 }}>
              <div className="frow" style={{ marginBottom: 8 }}>
                <input type="text" value={eName} onChange={(e) => setEName(e.target.value)} placeholder="班级名" style={{ flex: 1 }} />
                <input type="text" value={eTerm} onChange={(e) => setETerm(e.target.value)} placeholder="学期" style={{ width: 110 }} />
              </div>
              <div className="fl" style={{ marginBottom: 6 }}>教材</div>
              <TextbookPicker options={tbOptions} value={eTb} onChange={setETb} />
              <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
                <button className="primary sm" disabled={!eName} onClick={saveEdit}>保存</button>
                <button className="sm" onClick={() => setEditId(null)}>取消</button>
              </div>
            </div>
          ) : (
            <div className="row" key={c.id} style={{ flexWrap: "wrap" }}>
              <div>
                <div className="who">
                  {c.name}
                  {c.textbook &&
                    c.textbook.split(",").filter(Boolean).map((t) => (
                      <span key={t} className="tag makeup" style={{ marginLeft: 6 }}>{t}</span>
                    ))}
                </div>
                <div className="meta">{c.term ?? "—"} · {inClass(c.id).length} 人</div>
              </div>
              <div className="spacer" />
              {confirmDel === c.id ? (
                <>
                  <span className="muted" style={{ fontSize: 12 }}>删除方式：</span>
                  <button className="sm" onClick={() => delClassOnly(c.id)}>仅删班级（学生留）</button>
                  <button className="sm danger" onClick={() => delWithStudents(c.id)}>连学生一起删</button>
                  <button className="sm" onClick={() => setConfirmDel(null)}>取消</button>
                </>
              ) : (
                <>
                  <button className="sm" onClick={() => startEdit(c)}>改名 / 教材</button>
                  <button className="sm danger" onClick={() => setConfirmDel(c.id)}>删除</button>
                </>
              )}
            </div>
          ),
        )}
      </div>
    </Modal>
  );
}

/* ───────── 进班选学生 ───────── */
function PickStudentsModal({
  classId,
  className,
  students,
  clsName,
  onClose,
  onDone,
}: {
  classId: number;
  className: string;
  students: Student[];
  clsName: (id: number | null) => string;
  onClose: () => void;
  onDone: (msg: string) => void;
}) {
  const inThisClass = students.filter((s) => s.class_id === classId).map((s) => s.id);
  const [sel, setSel] = useState<number[]>(inThisClass);
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const toggle = (id: number) => setSel((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));
  const list = students.filter((s) => s.enabled && (!q || s.name.includes(q) || s.student_no.includes(q)));

  const save = async () => {
    setBusy(true);
    setErr("");
    try {
      const toAdd = sel.filter((id) => !inThisClass.includes(id));
      const toRemove = inThisClass.filter((id) => !sel.includes(id));
      if (toAdd.length) await studentsSetClass(toAdd, classId);
      if (toRemove.length) await studentsSetClass(toRemove, null);
      onDone(`「${className}」已更新：加入 ${toAdd.length} 人、移出 ${toRemove.length} 人`);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      title={`选择学生 → ${className}`}
      onClose={onClose}
      footer={
        <>
          <span className="sub" style={{ margin: 0 }}>本班 {sel.length} 人</span>
          <div style={{ display: "flex", gap: 10 }}>
            <button onClick={onClose}>取消</button>
            <button className="primary" disabled={busy} onClick={save}>{busy ? "保存中…" : "保存"}</button>
          </div>
        </>
      }
    >
      {err && <div className="error">{err}</div>}
      <div className="hint" style={{ marginBottom: 12 }}>勾选 = 在本班。勾别班的学生会把他从原班移过来（一个学生只属一个班）。</div>
      <input className="search" style={{ maxWidth: "100%", marginBottom: 10 }} placeholder="搜索 姓名 / 学号…" value={q} onChange={(e) => setQ(e.target.value)} />
      {list.length === 0 && <div className="muted" style={{ fontSize: 13 }}>没有学生，先去「批量导入」</div>}
      {list.map((s) => {
        const other = s.class_id != null && s.class_id !== classId;
        return (
          <label key={s.id} className="row" style={{ cursor: "pointer", marginBottom: 4 }}>
            <input type="checkbox" checked={sel.includes(s.id)} onChange={() => toggle(s.id)} />
            <div>
              <div className="who">{s.name}</div>
              <div className="meta">学号 {s.student_no}</div>
            </div>
            <div className="spacer" />
            {other && <span className="tag">现属 {clsName(s.class_id)}</span>}
          </label>
        );
      })}
    </Modal>
  );
}

/* ───────── 批量导入：支持多班级（班级名一行、学生几行） ───────── */
type ImpBlock = { className: string | null; students: { student_no: string; name: string }[] };
function parseImport(text: string): ImpBlock[] {
  const blocks: ImpBlock[] = [];
  let cur: ImpBlock = { className: null, students: [] };
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const parts = line.split(/[\t,，\s]+/).filter(Boolean);
    if (parts.length >= 2) {
      // 学号 + 姓名 → 学生
      cur.students.push({ student_no: parts[0], name: parts.slice(1).join("") });
    } else {
      // 单独一行 → 班级名
      if (cur.students.length || cur.className) blocks.push(cur);
      cur = { className: parts[0], students: [] };
    }
  }
  if (cur.students.length || cur.className) blocks.push(cur);
  return blocks.filter((b) => b.students.length > 0);
}

function ImportStudentsModal({ onClose, onDone }: { onClose: () => void; onDone: (stu: number, cls: number) => void }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [existingStudents, setExistingStudents] = useState<Student[]>([]);
  const [existingClasses, setExistingClasses] = useState<Class[]>([]);
  const blocks = useMemo(() => parseImport(text), [text]);
  const totalStu = blocks.reduce((s, b) => s + b.students.length, 0);
  const namedClasses = blocks.filter((b) => b.className).length;
  useEffect(() => {
    studentsList().then(setExistingStudents).catch(() => undefined);
    classesList().then(setExistingClasses).catch(() => undefined);
  }, []);
  const existingByNo = useMemo(
    () => new Map(existingStudents.map((s) => [s.student_no, s])),
    [existingStudents],
  );
  const classByName = useMemo(
    () => new Map(existingClasses.map((c) => [c.name, c])),
    [existingClasses],
  );
  const preview = useMemo(() => {
    let add = 0;
    let update = 0;
    let move = 0;
    let unassigned = 0;
    for (const b of blocks) {
      const target = b.className ? classByName.get(b.className)?.id ?? -1 : null;
      for (const s of b.students) {
        const old = existingByNo.get(s.student_no);
        if (!old) add += 1;
        else {
          update += 1;
          if (target !== old.class_id) move += 1;
        }
        if (b.className == null) unassigned += 1;
      }
    }
    return { add, update, move, unassigned };
  }, [blocks, existingByNo, classByName]);

  const save = async () => {
    setBusy(true);
    setErr("");
    try {
      // 1) 先把每个有名字的块建班，拿到 class_id
      const blockClassId: (number | null)[] = [];
      for (const b of blocks) {
        blockClassId.push(b.className ? (await classCreate(b.className)).id : null);
      }
      // 2) 一次性导入全部学生（upsert）
      const allRows = blocks.flatMap((b) => b.students);
      await studentsImport(allRows);
      // 3) 拉学生列表，按学号映射 id，逐班分班
      const all = await studentsList();
      const idByNo = new Map(all.map((s) => [s.student_no, s.id]));
      for (let i = 0; i < blocks.length; i++) {
        const cid = blockClassId[i];
        if (cid == null) continue;
        const ids = blocks[i].students.map((s) => idByNo.get(s.student_no)).filter((x): x is number => x != null);
        if (ids.length) await studentsSetClass(ids, cid);
      }
      onDone(totalStu, namedClasses);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      title="批量导入学生"
      onClose={onClose}
      footer={
        <>
          <span className="sub" style={{ margin: 0 }}>
            识别到 {namedClasses} 个班级、{totalStu} 名学生
          </span>
          <div style={{ display: "flex", gap: 10 }}>
            <button onClick={onClose}>取消</button>
            <button className="primary" disabled={busy || !totalStu} onClick={save}>
              {busy ? "导入中…" : "导入"}
            </button>
          </div>
        </>
      }
    >
      {err && <div className="error">{err}</div>}
      <div className="hint" style={{ marginBottom: 12 }}>
        <b>格式：班级名单独一行，下面跟该班学生（每行 学号 + Tab/空格/逗号 + 姓名）。</b>
        可一次导入多个班级；不写班级名的学生进「未分班」。建好的班会自动建。
      </div>
      {totalStu > 0 && (
        <div className="ok-banner" style={{ margin: "0 0 12px" }}>
          预览：新增 {preview.add} 名，更新 {preview.update} 名，调整分班 {preview.move} 名，进入未分班 {preview.unassigned} 名
        </div>
      )}
      <div className="frow" style={{ alignItems: "flex-start", gap: 14 }}>
        <div className="field" style={{ flex: 1 }}>
          <div className="fl">粘贴名单</div>
          <textarea
            rows={12}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={"八(1)班\n2023001\t张明\n2023002\t李华\n八(2)班\n2023021\t刘洋\n2023022\t孙莉"}
          />
        </div>
        {blocks.length > 0 && (
          <div style={{ width: 200, flexShrink: 0 }}>
            <div className="fl" style={{ marginBottom: 6 }}>预览</div>
            {blocks.map((b, i) => (
              <div key={i} style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 13, fontWeight: 600 }}>{b.className ?? "未分班"}</div>
                <div className="meta">{b.students.length} 人：{b.students.slice(0, 4).map((s) => s.name).join("、")}{b.students.length > 4 ? "…" : ""}</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </Modal>
  );
}
