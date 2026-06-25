import { useCallback, useEffect, useState } from "react";
import {
  contentsList,
  contentsUpsert,
  studentsList,
  studentsUpsert,
  tasksGenerate,
  type RecContent,
  type Student,
} from "../api/manage";

export default function Manage() {
  const [students, setStudents] = useState<Student[]>([]);
  const [contents, setContents] = useState<RecContent[]>([]);
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");

  // 新建表单
  const [sNo, setSNo] = useState("");
  const [sName, setSName] = useState("");
  const [cNo, setCNo] = useState("");
  const [cTitle, setCTitle] = useState("");
  const [cAnswer, setCAnswer] = useState("");

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
    window.setTimeout(() => setMsg(""), 3000);
  };

  const addStudent = async () => {
    if (!sNo || !sName) return;
    try {
      await studentsUpsert(sNo, sName, true);
      setSNo("");
      setSName("");
      await refresh();
      flash("学生已保存");
    } catch (e) {
      setErr(String(e));
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
      setErr(String(e));
    }
  };

  // 给所有启用学生 × 选中内容生成今日新背任务
  const genFor = async (contentId: number) => {
    const pairs: [number, number][] = students
      .filter((s) => s.enabled)
      .map((s) => [s.id, contentId]);
    if (pairs.length === 0) {
      setErr("没有启用的学生");
      return;
    }
    try {
      const n = await tasksGenerate(pairs);
      flash(`已生成 ${n} 条今日新背任务`);
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <div className="page">
      <h1>管理</h1>
      {err && <div className="error">出错：{err}</div>}
      {msg && <div className="ok-banner">{msg}</div>}

      <section className="group">
        <h2>学生 <span className="badge">{students.length}</span></h2>
        <div className="form row">
          <input placeholder="学号" value={sNo} onChange={(e) => setSNo(e.target.value)} />
          <input placeholder="姓名" value={sName} onChange={(e) => setSName(e.target.value)} />
          <button className="primary" onClick={addStudent}>添加/更新</button>
        </div>
        <table className="tbl">
          <thead>
            <tr><th>学号</th><th>姓名</th><th>状态</th></tr>
          </thead>
          <tbody>
            {students.map((s) => (
              <tr key={s.id}>
                <td>{s.student_no}</td>
                <td>{s.name}</td>
                <td>{s.enabled ? "启用" : "停用"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="group">
        <h2>背诵内容 <span className="badge">{contents.length}</span></h2>
        <div className="form">
          <div className="row">
            <input placeholder="编号 如 C012" value={cNo} onChange={(e) => setCNo(e.target.value)} />
            <input placeholder="标题 如 静夜思" value={cTitle} onChange={(e) => setCTitle(e.target.value)} />
          </div>
          <textarea placeholder="标准答案全文" rows={3} value={cAnswer} onChange={(e) => setCAnswer(e.target.value)} />
          <button className="primary" onClick={addContent}>添加/更新</button>
        </div>
        <table className="tbl">
          <thead>
            <tr><th>编号</th><th>标题</th><th>版本</th><th>操作</th></tr>
          </thead>
          <tbody>
            {contents.map((c) => (
              <tr key={c.id}>
                <td>{c.content_no}</td>
                <td>{c.title}</td>
                <td>v{c.answer_version}</td>
                <td><button onClick={() => genFor(c.id)}>生成今日新背</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}
