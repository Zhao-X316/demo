import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  asrAndScore,
  importAutoname,
  importPaths,
  importStage,
  type AutonameResult,
  type ImportResult,
  type StageResult,
} from "../api/importing";
import { AudioPlayer } from "../components/AudioPlayer";

interface Row extends ImportResult {
  asr?: string;
  asrBusy?: boolean;
}

const AUDIO_FILTER = [{ name: "音频", extensions: ["m4a", "mp3", "wav", "ogg", "aac", "flac"] }];

export default function Import() {
  // 智能识别（两段式）：staged 队列 → 分析结果
  const [staged, setStaged] = useState<StageResult[]>([]);
  const [auto, setAuto] = useState<AutonameResult[]>([]);
  const [analyzing, setAnalyzing] = useState(false);
  // 按文件名导入（备用路径）
  const [rows, setRows] = useState<Row[]>([]);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const queued = staged.filter((s) => s.status === "staged");

  // 第一步「导入」：选文件 → 哈希去重入队（不调 ASR）。可多次追加。
  const pickAndStage = async () => {
    setErr("");
    try {
      const sel = await open({ multiple: true, filters: AUDIO_FILTER });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      setBusy(true);
      const res = await importStage(paths);
      setStaged((prev) => {
        const seen = new Set(prev.map((x) => x.file));
        return [...prev, ...res.filter((r) => !seen.has(r.file))];
      });
      setAuto([]);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  // 第二步「开始分析」：对队列里 staged 的文件批量 ASR + 识别 + 改名 + 评分
  const analyze = async () => {
    if (queued.length === 0) return;
    setErr("");
    setAnalyzing(true);
    try {
      const res = await importAutoname(queued.map((s) => s.file));
      setAuto(res);
      setStaged([]);
    } catch (e) {
      setErr(String(e));
    } finally {
      setAnalyzing(false);
    }
  };

  const clearQueue = () => {
    setStaged([]);
    setAuto([]);
  };

  // 按文件名导入（文件名已是 日期_学号_姓名_编号）
  const nameImport = async () => {
    setErr("");
    try {
      const sel = await open({ multiple: true, filters: AUDIO_FILTER });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      setBusy(true);
      setRows(await importPaths(paths));
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
      </div>

      {err && <div className="error">出错：{err}</div>}

      {/* 智能识别导入：两段式 */}
      <section className="group">
        <h2>智能识别导入</h2>
        <p className="muted">
          录音开头说「姓名 + 日期 + 背诵内容」。第一步先把录音<b>全部导入</b>进队列，
          第二步点<b>「开始分析」</b>才统一识别、命名为 <code>日期_学号_姓名_编号</code> 并评分。
        </p>
        <div className="row">
          <button onClick={pickAndStage} disabled={busy || analyzing}>
            {busy ? "导入中…" : "选择并导入"}
          </button>
          <button className="primary" onClick={analyze} disabled={analyzing || queued.length === 0}>
            {analyzing ? "分析中…" : `开始分析${queued.length ? ` (${queued.length})` : ""}`}
          </button>
          {staged.length > 0 && (
            <button className="link" onClick={clearQueue} disabled={analyzing}>
              清空队列
            </button>
          )}
        </div>

        {/* 待分析队列 */}
        {staged.length > 0 && (
          <table className="tbl">
            <thead>
              <tr><th>待分析队列（{queued.length}）</th><th>状态</th></tr>
            </thead>
            <tbody>
              {staged.map((s, i) => (
                <tr key={i}>
                  <td className="filecell">
                    {baseName(s.file)}
                    {s.status === "staged" && <AudioPlayer path={s.file} />}
                  </td>
                  <td>
                    <span className={`pill pill-${stagePill(s.status)}`}>{stageLabel(s.status)}</span>
                    {s.status !== "staged" && <span className="muted"> {s.detail}</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {/* 分析结果 */}
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
                    {r.status === "unmatched" && (
                      <div className="muted">→ 去「异常池」页填学生/内容后改派即可</div>
                    )}
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
      </section>

      {/* 按文件名导入（备用） */}
      <section className="group">
        <h2>按文件名导入</h2>
        <p className="muted">
          文件名已是 <code>日期_学号_姓名_编号</code> 时用这个；导入后逐条「识别并评分」。
        </p>
        <button onClick={nameImport} disabled={busy || analyzing}>选择文件导入</button>
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
      </section>
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
function stageLabel(s: string): string {
  const m: Record<string, string> = { staged: "已排队", duplicate: "重复(跳过)", error: "错误" };
  return m[s] ?? s;
}
function stagePill(s: string): string {
  if (s === "staged") return "submitted";
  return "failed";
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
