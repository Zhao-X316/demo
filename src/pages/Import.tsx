import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { asrAndScore, importPaths, type ImportResult } from "../api/importing";
import { AudioPlayer } from "../components/AudioPlayer";

interface Row extends ImportResult {
  asr?: string; // 识别评分结果摘要
  asrBusy?: boolean;
}

export default function Import() {
  const [rows, setRows] = useState<Row[]>([]);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const pick = async () => {
    setErr("");
    try {
      const sel = await open({
        multiple: true,
        filters: [{ name: "音频", extensions: ["m4a", "mp3", "wav", "ogg", "aac", "flac"] }],
      });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      setBusy(true);
      const res = await importPaths(paths);
      setRows(res);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runAsr = async (idx: number) => {
    const r = rows[idx];
    if (!r.submission_id) return;
    setRows((rs) => rs.map((x, i) => (i === idx ? { ...x, asrBusy: true } : x)));
    try {
      const o = await asrAndScore(r.submission_id);
      const summary = `正确率 ${Math.round(o.accuracy)}% ${o.pass ? "✓通过" : "✗未达标"} · 熟练度 ${Math.round(o.fluency)}(${o.quality})`;
      setRows((rs) => rs.map((x, i) => (i === idx ? { ...x, asr: summary, asrBusy: false } : x)));
    } catch (e) {
      setRows((rs) => rs.map((x, i) => (i === idx ? { ...x, asr: `识别失败：${String(e)}`, asrBusy: false } : x)));
    }
  };

  const runAll = async () => {
    for (let i = 0; i < rows.length; i++) {
      if (rows[i].status === "imported" && rows[i].submission_id && !rows[i].asr) {
        // 顺序执行，避免并发打满火山额度
        // eslint-disable-next-line no-await-in-loop
        await runAsr(i);
      }
    }
  };

  const importedCount = rows.filter((r) => r.status === "imported").length;

  return (
    <div className="page">
      <div className="page-head">
        <h1>导入录音</h1>
        <div className="row">
          <button className="primary" onClick={pick} disabled={busy}>选择音频文件</button>
          {importedCount > 0 && <button onClick={runAll} disabled={busy}>全部识别评分</button>}
        </div>
      </div>
      <p className="muted">文件名格式：<code>日期_学号_姓名_内容编号</code>，如 <code>20260625_2023001_张三_C012.m4a</code></p>
      {err && <div className="error">出错：{err}</div>}

      {rows.length > 0 && (
        <table className="tbl">
          <thead>
            <tr><th>文件</th><th>导入</th><th>说明</th><th>识别评分</th></tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i}>
                <td className="filecell">
                  {baseName(r.file)}
                  {r.status !== "error" && <AudioPlayer path={r.file} />}
                </td>
                <td><span className={`pill pill-${pillClass(r.status)}`}>{statusLabel(r.status)}</span></td>
                <td className="muted">{r.detail}</td>
                <td>
                  {r.status === "imported" && r.submission_id ? (
                    r.asr ? (
                      <span className="muted">{r.asr}</span>
                    ) : (
                      <button disabled={r.asrBusy} onClick={() => runAsr(i)}>
                        {r.asrBusy ? "识别中…" : "识别并评分"}
                      </button>
                    )
                  ) : (
                    <span className="muted">—</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function baseName(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}
function statusLabel(s: string): string {
  const m: Record<string, string> = { imported: "已导入", duplicate: "重复", anomaly: "异常", error: "错误" };
  return m[s] ?? s;
}
function pillClass(s: string): string {
  if (s === "imported") return "passed";
  if (s === "anomaly" || s === "error") return "failed";
  return "submitted";
}
