import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  asrAndScore,
  importAutoname,
  importPaths,
  type AutonameResult,
  type ImportResult,
} from "../api/importing";
import { AudioPlayer } from "../components/AudioPlayer";

interface Row extends ImportResult {
  asr?: string;
  asrBusy?: boolean;
}

const AUDIO_FILTER = [{ name: "音频", extensions: ["m4a", "mp3", "wav", "ogg", "aac", "flac"] }];

export default function Import() {
  const [rows, setRows] = useState<Row[]>([]);
  const [auto, setAuto] = useState<AutonameResult[]>([]);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  // 智能识别导入：分析录音内容 → 识别学生/内容 → 自动改名评分
  const smartImport = async () => {
    setErr("");
    try {
      const sel = await open({ multiple: true, filters: AUDIO_FILTER });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      setBusy(true);
      setAuto(await importAutoname(paths));
      setRows([]);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  // 按文件名导入（备用：文件名已是 日期_学号_姓名_编号）
  const nameImport = async () => {
    setErr("");
    try {
      const sel = await open({ multiple: true, filters: AUDIO_FILTER });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      setBusy(true);
      setRows(await importPaths(paths));
      setAuto([]);
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

  return (
    <div className="page">
      <div className="page-head">
        <h1>导入录音</h1>
        <div className="row">
          <button className="primary" onClick={smartImport} disabled={busy}>
            {busy ? "处理中…" : "智能识别导入"}
          </button>
          <button onClick={nameImport} disabled={busy}>按文件名导入</button>
        </div>
      </div>
      <p className="muted">
        智能识别：录音开头说「姓名 + 日期 + 背诵内容」，系统自动识别、命名为
        <code>日期_学号_姓名_编号</code> 并评分。
      </p>
      {err && <div className="error">出错：{err}</div>}

      {auto.length > 0 && (
        <table className="tbl">
          <thead>
            <tr><th>原文件</th><th>识别结果</th><th>学生 / 内容</th><th>评分</th></tr>
          </thead>
          <tbody>
            {auto.map((r, i) => (
              <tr key={i}>
                <td className="filecell">
                  {baseName(r.file)}
                  {r.status !== "error" && <AudioPlayer path={r.file} />}
                </td>
                <td>
                  <span className={`pill pill-${pillClass(r.status)}`}>{autoLabel(r.status)}</span>
                  {r.new_name && <div className="muted">→ {r.new_name}</div>}
                  {r.status !== "scored" && <div className="muted">{r.detail}</div>}
                </td>
                <td className="muted">
                  {r.student ?? "—"}
                  {r.content ? <div>{r.content}</div> : null}
                </td>
                <td>
                  {r.accuracy != null ? (
                    <span className={r.pass ? "acc ok" : "acc bad"}>
                      {Math.round(r.accuracy)}% {r.pass ? "✓" : "✗"}
                    </span>
                  ) : (
                    "—"
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

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
function autoLabel(s: string): string {
  const m: Record<string, string> = { scored: "已评分", duplicate: "重复", unmatched: "未识别", error: "错误" };
  return m[s] ?? s;
}
function pillClass(s: string): string {
  if (s === "imported" || s === "scored") return "passed";
  if (s === "anomaly" || s === "error" || s === "unmatched") return "failed";
  return "submitted";
}
