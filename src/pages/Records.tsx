import { useEffect, useMemo, useState } from "react";
import { ImportHistoryRow, importHistory } from "../api/records";

export default function Records() {
  const [rows, setRows] = useState<ImportHistoryRow[] | null>(null);
  const [q, setQ] = useState("");
  const [err, setErr] = useState("");

  useEffect(() => {
    importHistory()
      .then(setRows)
      .catch((e) => setErr(String(e)));
  }, []);

  const filtered = useMemo(
    () =>
      (rows ?? []).filter(
        (r) =>
          !q ||
          r.file_name.includes(q) ||
          (r.student ?? "").includes(q) ||
          (r.content ?? "").includes(q),
      ),
    [rows, q],
  );

  return (
    <div className="page">
      <div className="page-head">
        <h1>记录</h1>
        <span className="date">导入历史 · 共 {rows?.length ?? 0} 个文件</span>
      </div>
      <div className="sub">
        所有导入过的录音都留底（切走再回来不丢）。原文件名经智能识别后改成 <code>日期_学号_姓名_编号</code>。
      </div>
      {err && <div className="error">{err}</div>}
      <div className="toolbar">
        <input className="search" placeholder="搜索 文件名 / 学生 / 内容…" value={q} onChange={(e) => setQ(e.target.value)} />
        <div className="spacer" />
        <span className="muted" style={{ fontSize: 13 }}>显示 {filtered.length} 条</span>
      </div>

      {rows == null && <div className="loading">加载导入历史…</div>}
      {rows != null && (
        <table className="tbl">
          <thead>
            <tr>
              <th>文件名（识别后）</th>
              <th>学生</th>
              <th>内容</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((r) => (
              <tr key={r.submission_id}>
                <td className="filecell">{r.file_name}</td>
                <td>{r.student ?? "—"}</td>
                <td>{r.content ?? "—"}</td>
                <td style={{ color: r.status.includes("未识别") ? "var(--bad)" : "var(--text-2)" }}>
                  {r.status}
                </td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={4} className="muted" style={{ padding: "24px 8px", textAlign: "center" }}>
                  暂无记录
                </td>
              </tr>
            )}
          </tbody>
        </table>
      )}
    </div>
  );
}
