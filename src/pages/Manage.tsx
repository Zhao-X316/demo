import { useCallback, useEffect, useState } from "react";
import {
  contentsDelete,
  contentsImport,
  contentsList,
  contentsSetEnabled,
  contentsUpsert,
  parseSyllabus,
  studentsDelete,
  studentsImport,
  studentsList,
  studentsSetEnabled,
  studentsUpsert,
  tasksGenerate,
  type ParsedContent,
  type RecContent,
  type Student,
} from "../api/manage";

function splitLine(l: string): string[] {
  return l.split(/\t|,/).map((x) => x.trim());
}
function parseStudents(text: string): { student_no: string; name: string }[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => {
      const p = splitLine(l);
      return { student_no: p[0] ?? "", name: p[1] ?? "" };
    })
    .filter((r) => r.student_no && r.name);
}
function parseContents(text: string): { content_no: string; title: string; answer_text: string }[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => {
      const p = splitLine(l);
      return { content_no: p[0] ?? "", title: p[1] ?? "", answer_text: p.slice(2).join(" ").trim() };
    })
    .filter((r) => r.content_no && r.title && r.answer_text);
}

export default function Manage() {
  const [students, setStudents] = useState<Student[]>([]);
  const [contents, setContents] = useState<RecContent[]>([]);
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");

  // 单条新建
  const [sNo, setSNo] = useState("");
  const [sName, setSName] = useState("");
  const [cNo, setCNo] = useState("");
  const [cTitle, setCTitle] = useState("");
  const [cAnswer, setCAnswer] = useState("");

  // 多选
  const [selS, setSelS] = useState<Set<number>>(new Set());
  const [selC, setSelC] = useState<Set<number>>(new Set());
  // 批量导入文本
  const [impS, setImpS] = useState("");
  const [impC, setImpC] = useState("");

  const refresh = useCallback(async () => {
    try {
      setStudents(await studentsList());
      setContents(await contentsList());
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const flash = (m: string) => {
    setMsg(m);
    window.setTimeout(() => setMsg(""), 4000);
  };
  const fail = (e: unknown) => setErr(String(e));

  const addStudent = async () => {
    if (!sNo || !sName) return;
    try {
      await studentsUpsert(sNo, sName, true);
      setSNo("");
      setSName("");
      await refresh();
      flash("学生已保存");
    } catch (e) {
      fail(e);
    }
  };

  const addContent = async () => {
    if (!cNo || !cTitle || !cAnswer) return;
    try {
      await contentsUpsert(cNo, cTitle, cAnswer, true);
      setCNo("");
      setCTitle("");
      setCAnswer("");
      await refresh();
      flash("内容已保存");
    } catch (e) {
      fail(e);
    }
  };

  const genFor = async (contentId: number) => {
    const pairs: [number, number][] = students.filter((s) => s.enabled).map((s) => [s.id, contentId]);
    if (pairs.length === 0) {
      setErr("没有启用的学生");
      return;
    }
    try {
      const n = await tasksGenerate(pairs);
      flash(`已生成 ${n} 条今日新背任务`);
    } catch (e) {
      fail(e);
    }
  };

  // —— 批量导入 ——
  const importStudents = async () => {
    const rows = parseStudents(impS);
    if (rows.length === 0) {
      setErr("没有解析到有效的「学号 + 姓名」行");
      return;
    }
    try {
      const r = await studentsImport(rows);
      setImpS("");
      await refresh();
      flash(`导入学生：成功 ${r.ok}${r.failed ? `，失败 ${r.failed}（${r.errors[0] ?? ""}…）` : ""}`);
    } catch (e) {
      fail(e);
    }
  };
  const importContents = async () => {
    const rows = parseContents(impC);
    if (rows.length === 0) {
      setErr("没有解析到有效的「编号 + 标题 + 答案」行");
      return;
    }
    try {
      const r = await contentsImport(rows);
      setImpC("");
      await refresh();
      flash(`导入内容：成功 ${r.ok}${r.failed ? `，失败 ${r.failed}（${r.errors[0] ?? ""}…）` : ""}`);
    } catch (e) {
      fail(e);
    }
  };

  // —— 批量删减 ——
  const studentsAction = async (act: "enable" | "disable" | "delete") => {
    const ids = [...selS];
    if (ids.length === 0) return;
    try {
      if (act === "delete") {
        if (!window.confirm(`确定删除选中的 ${ids.length} 名学生？有历史记录的会自动改为「停用」。`)) return;
        const r = await studentsDelete(ids);
        flash(
          `删除 ${r.deleted} 名${r.blocked.length ? `；${r.blocked.length} 名有历史记录未删（建议停用）：${r.blocked.join("、")}` : ""}`,
        );
      } else {
        const n = await studentsSetEnabled(ids, act === "enable");
        flash(`${act === "enable" ? "启用" : "停用"} ${n} 名学生`);
      }
      setSelS(new Set());
      await refresh();
    } catch (e) {
      fail(e);
    }
  };
  const contentsAction = async (act: "enable" | "disable" | "delete") => {
    const ids = [...selC];
    if (ids.length === 0) return;
    try {
      if (act === "delete") {
        if (!window.confirm(`确定删除选中的 ${ids.length} 项内容？有历史记录的会自动改为「停用」。`)) return;
        const r = await contentsDelete(ids);
        flash(
          `删除 ${r.deleted} 项${r.blocked.length ? `；${r.blocked.length} 项有历史记录未删（建议停用）：${r.blocked.join("、")}` : ""}`,
        );
      } else {
        const n = await contentsSetEnabled(ids, act === "enable");
        flash(`${act === "enable" ? "启用" : "停用"} ${n} 项内容`);
      }
      setSelC(new Set());
      await refresh();
    } catch (e) {
      fail(e);
    }
  };

  const toggle = (set: Set<number>, setter: (s: Set<number>) => void, id: number) => {
    const n = new Set(set);
    if (n.has(id)) n.delete(id);
    else n.add(id);
    setter(n);
  };
  const toggleAll = (ids: number[], set: Set<number>, setter: (s: Set<number>) => void) => {
    setter(set.size === ids.length ? new Set() : new Set(ids));
  };

  return (
    <div className="page">
      <h1>管理</h1>
      {err && <div className="error">出错：{err}</div>}
      {msg && <div className="ok-banner">{msg}</div>}

      {/* 学生 */}
      <section className="group">
        <h2>
          学生 <span className="badge">{students.length}</span>
        </h2>
        <div className="form row">
          <input placeholder="学号" value={sNo} onChange={(e) => setSNo(e.target.value)} />
          <input placeholder="姓名" value={sName} onChange={(e) => setSName(e.target.value)} />
          <button className="primary" onClick={addStudent}>添加/更新</button>
        </div>
        <details className="batch">
          <summary>批量导入（粘贴，每行：学号 + Tab/逗号 + 姓名）</summary>
          <textarea
            rows={4}
            placeholder={"2023001\t张三\n2023002\t李四"}
            value={impS}
            onChange={(e) => setImpS(e.target.value)}
          />
          <button className="primary" onClick={importStudents}>批量导入学生</button>
        </details>

        {selS.size > 0 && (
          <div className="filter-bar">
            已选 <b>{selS.size}</b> 名：
            <button className="link" onClick={() => studentsAction("disable")}>停用</button>
            <button className="link" onClick={() => studentsAction("enable")}>启用</button>
            <button className="link danger" onClick={() => studentsAction("delete")}>删除</button>
            <button className="link" onClick={() => setSelS(new Set())}>取消选择</button>
          </div>
        )}
        <table className="tbl">
          <thead>
            <tr>
              <th className="ckcol">
                <input
                  type="checkbox"
                  checked={students.length > 0 && selS.size === students.length}
                  onChange={() => toggleAll(students.map((s) => s.id), selS, setSelS)}
                />
              </th>
              <th>学号</th>
              <th>姓名</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            {students.map((s) => (
              <tr key={s.id} className={s.enabled ? "" : "disabled-row"}>
                <td className="ckcol">
                  <input type="checkbox" checked={selS.has(s.id)} onChange={() => toggle(selS, setSelS, s.id)} />
                </td>
                <td>{s.student_no}</td>
                <td>{s.name}</td>
                <td>{s.enabled ? "启用" : "停用"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      {/* 背诵内容 */}
      <section className="group">
        <h2>
          背诵内容 <span className="badge">{contents.length}</span>
        </h2>
        <div className="form">
          <div className="row">
            <input placeholder="编号 如 C012" value={cNo} onChange={(e) => setCNo(e.target.value)} />
            <input placeholder="标题 如 静夜思" value={cTitle} onChange={(e) => setCTitle(e.target.value)} />
          </div>
          <textarea placeholder="标准答案全文" rows={3} value={cAnswer} onChange={(e) => setCAnswer(e.target.value)} />
          <button className="primary" onClick={addContent}>添加/更新</button>
        </div>
        <details className="batch">
          <summary>批量导入（粘贴，每行：编号 + Tab/逗号 + 标题 + Tab/逗号 + 答案）</summary>
          <textarea
            rows={4}
            placeholder={"C012\t静夜思\t床前明月光，疑是地上霜。\nC013\t春晓\t春眠不觉晓，处处闻啼鸟。"}
            value={impC}
            onChange={(e) => setImpC(e.target.value)}
          />
          <button className="primary" onClick={importContents}>批量导入内容</button>
        </details>
        <SyllabusImport onImported={refresh} />

        {selC.size > 0 && (
          <div className="filter-bar">
            已选 <b>{selC.size}</b> 项：
            <button className="link" onClick={() => contentsAction("disable")}>停用</button>
            <button className="link" onClick={() => contentsAction("enable")}>启用</button>
            <button className="link danger" onClick={() => contentsAction("delete")}>删除</button>
            <button className="link" onClick={() => setSelC(new Set())}>取消选择</button>
          </div>
        )}
        <table className="tbl">
          <thead>
            <tr>
              <th className="ckcol">
                <input
                  type="checkbox"
                  checked={contents.length > 0 && selC.size === contents.length}
                  onChange={() => toggleAll(contents.map((c) => c.id), selC, setSelC)}
                />
              </th>
              <th>编号</th>
              <th>标题</th>
              <th>版本</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {contents.map((c) => (
              <tr key={c.id} className={c.enabled ? "" : "disabled-row"}>
                <td className="ckcol">
                  <input type="checkbox" checked={selC.has(c.id)} onChange={() => toggle(selC, setSelC, c.id)} />
                </td>
                <td>{c.content_no}</td>
                <td>{c.title}</td>
                <td>v{c.answer_version}</td>
                <td>{c.enabled ? "启用" : "停用"}</td>
                <td><button onClick={() => genFor(c.id)}>生成今日新背</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}

interface SylRow extends ParsedContent {
  include: boolean;
}

function SyllabusImport({ onImported }: { onImported: () => void }) {
  const [prefix, setPrefix] = useState("");
  const [text, setText] = useState("");
  const [rows, setRows] = useState<SylRow[]>([]);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");

  const parse = async () => {
    if (!prefix.trim()) return setNote("先填学科册前缀，如 道法8上");
    if (!text.trim()) return setNote("先粘贴清单文本");
    setBusy(true);
    setNote("");
    try {
      const r = await parseSyllabus(text, prefix.trim());
      setRows(r.map((x) => ({ ...x, include: true })));
      setNote(`解析出 ${r.length} 条，检查/改标题/取消勾选后导入`);
    } catch (e) {
      setNote("解析失败：" + String(e));
    } finally {
      setBusy(false);
    }
  };

  const setTitle = (i: number, v: string) =>
    setRows((rs) => rs.map((x, j) => (j === i ? { ...x, title: v } : x)));
  const toggle = (i: number) =>
    setRows((rs) => rs.map((x, j) => (j === i ? { ...x, include: !x.include } : x)));

  const included = rows.filter((r) => r.include).length;

  const doImport = async () => {
    const sel = rows.filter((r) => r.include);
    if (sel.length === 0) return setNote("没有勾选的条目");
    setBusy(true);
    setNote("");
    try {
      const r = await contentsImport(
        sel.map((x) => ({
          content_no: x.content_no,
          title: x.is_key ? "★" + x.title : x.title,
          answer_text: x.answer_text,
        })),
      );
      setRows([]);
      setText("");
      onImported();
      setNote(`导入成功 ${r.ok}${r.failed ? `，失败 ${r.failed}（${r.errors[0] ?? ""}…）` : ""}`);
    } catch (e) {
      setNote("导入失败：" + String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <details className="batch">
      <summary>从背诵清单导入（粘贴「第X课 → 1．2．点」清单，自动按点切分）</summary>
      <div className="row">
        <input placeholder="学科册前缀 如 道法8上" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
        <button disabled={busy} onClick={parse}>{busy ? "处理中…" : "解析预览"}</button>
      </div>
      <textarea
        rows={4}
        placeholder="粘贴整份背诵清单文本（支持 .md/.txt/直接粘贴）…"
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      {note && <div className="muted">{note}</div>}
      {rows.length > 0 && (
        <>
          <div className="filter-bar">
            解析 <b>{rows.length}</b> 条 · 勾选 <b>{included}</b> 条
            <button className="primary sm" disabled={busy} onClick={doImport}>确认导入 {included} 条</button>
          </div>
          <table className="tbl">
            <thead>
              <tr>
                <th className="ckcol"></th>
                <th>编号</th>
                <th>标题（可改）</th>
                <th>答案</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={i} className={r.include ? "" : "disabled-row"}>
                  <td className="ckcol">
                    <input type="checkbox" checked={r.include} onChange={() => toggle(i)} />
                  </td>
                  <td>
                    {r.content_no}
                    {r.is_key && " ★"}
                  </td>
                  <td>
                    <input value={r.title} onChange={(e) => setTitle(i, e.target.value)} style={{ width: "100%" }} />
                  </td>
                  <td>
                    <details>
                      <summary className="muted">展开</summary>
                      <div className="muted">{r.answer_text}</div>
                    </details>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </details>
  );
}
