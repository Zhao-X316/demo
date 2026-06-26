import { useCallback, useEffect, useState } from "react";
import { anomaliesList, anomalyReassign, suggestMatch, type Anomaly, type Suggest } from "../api/anomaly";
import { asrAndScore } from "../api/importing";
import { contentsList, studentsList, type RecContent, type Student } from "../api/manage";
import { AudioPlayer } from "../components/AudioPlayer";

// 从 parsed_meta JSON 里取出 ASR 识别原文（autoname_unmatched 会写 {"asr": "..."}）。
function asrText(meta: string | null): string {
  if (!meta) return "";
  try {
    const o = JSON.parse(meta) as { asr?: unknown };
    return typeof o.asr === "string" ? o.asr : "";
  } catch {
    return "";
  }
}

export default function AnomalyPage() {
  const [rows, setRows] = useState<Anomaly[]>([]);
  const [students, setStudents] = useState<Student[]>([]);
  const [contents, setContents] = useState<RecContent[]>([]);
  const [suggest, setSuggest] = useState<Record<number, Suggest>>({});
  const [edit, setEdit] = useState<Record<number, { sno: string; cno: string }>>({});
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [a, s, c] = await Promise.all([anomaliesList(), studentsList(), contentsList()]);
      setRows(a);
      setStudents(s);
      setContents(c);
      setErr("");
      // 为每条异常算候选建议（基于已存的 ASR 原文）
      const sug: Record<number, Suggest> = {};
      await Promise.all(
        a.map(async (r) => {
          const t = asrText(r.parsed_meta);
          if (t) {
            try {
              sug[r.submission_id] = await suggestMatch(t);
            } catch {
              /* 建议失败不影响手动改派 */
            }
          }
        }),
      );
      setSuggest(sug);
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

  const field = (id: number, k: "sno" | "cno", v: string) =>
    setEdit((s) => ({
      ...s,
      [id]: { sno: s[id]?.sno ?? "", cno: s[id]?.cno ?? "", [k]: v },
    }));

  // 核心改派 + 识别评分
  const doReassign = async (id: number, sno: string, cno: string, label: string) => {
    if (!sno && !cno) {
      setErr("请至少选择 学生 或 背诵内容");
      return;
    }
    setErr("");
    setBusy(true);
    try {
      await anomalyReassign(id, sno || undefined, cno || undefined);
    } catch (e) {
      setErr("改派失败：" + String(e));
      setBusy(false);
      return;
    }
    try {
      const o = await asrAndScore(id);
      flash(`${label}：正确率 ${Math.round(o.accuracy)}% ${o.pass ? "通过" : "未达标"}`);
    } catch (e) {
      flash("已改派；识别评分待办（" + String(e) + "）");
    }
    await refresh();
    setBusy(false);
  };

  const reassignManual = (id: number) => {
    const f = edit[id] ?? { sno: "", cno: "" };
    void doReassign(id, f.sno, f.cno, "已改派并识别");
  };
  const adoptAndScore = (id: number) => {
    const sg = suggest[id];
    void doReassign(id, sg?.student_no ?? "", sg?.contents[0]?.content_no ?? "", "已采纳建议并识别");
  };

  return (
    <div className="page">
      <div className="page-head">
        <h1>异常池 <span className="badge">{rows.length}</span></h1>
        <button onClick={() => void refresh()} disabled={busy}>刷新</button>
      </div>
      <p className="muted">
        系统识别不准的录音落在这里。看「识别原文」+ 系统给的<b>候选建议</b>，确认后一键采纳，或手动选学生/内容改派。
      </p>
      {err && <div className="error">出错：{err}</div>}
      {msg && <div className="ok-banner">{msg}</div>}
      {rows.length === 0 && !err && <p className="muted">没有异常项 🎉</p>}

      {rows.map((r) => {
        const asr = asrText(r.parsed_meta);
        const sg = suggest[r.submission_id];
        return (
          <div className="card" key={r.submission_id}>
            <div className="card-top">
              <span className="pill pill-failed">{r.anomaly_type}</span>
              <span className="muted">{baseName(r.file_path)}</span>
            </div>
            {asr ? (
              <div className="asr">识别原文：{asr}</div>
            ) : (
              r.parsed_meta && <div className="muted">解析：{r.parsed_meta}</div>
            )}
            <AudioPlayer path={r.file_path} />

            {/* 候选建议 */}
            {sg && (sg.student_no || sg.contents.length > 0) && (
              <div className="suggest">
                <span className="muted">建议：</span>
                {sg.student_no && (
                  <button
                    className="chip"
                    disabled={busy}
                    onClick={() => field(r.submission_id, "sno", sg.student_no!)}
                    title="点此填入学生"
                  >
                    👤 {sg.student_no} {sg.student_name}
                  </button>
                )}
                {sg.contents.map((c) => (
                  <button
                    key={c.content_no}
                    className={`chip ${c.score >= 50 ? "chip-ok" : ""}`}
                    disabled={busy}
                    onClick={() => field(r.submission_id, "cno", c.content_no)}
                    title="点此填入内容"
                  >
                    📖 {c.content_no} {c.title} {Math.round(c.score)}%
                  </button>
                ))}
                <button className="primary sm" disabled={busy} onClick={() => adoptAndScore(r.submission_id)}>
                  一键采纳并识别
                </button>
              </div>
            )}

            <div className="row" style={{ marginTop: 10 }}>
              <select
                value={edit[r.submission_id]?.sno ?? ""}
                onChange={(ev) => field(r.submission_id, "sno", ev.target.value)}
              >
                <option value="">选择学生…</option>
                {students.map((s) => (
                  <option key={s.id} value={s.student_no}>
                    {s.student_no} {s.name}
                  </option>
                ))}
              </select>
              <select
                value={edit[r.submission_id]?.cno ?? ""}
                onChange={(ev) => field(r.submission_id, "cno", ev.target.value)}
              >
                <option value="">选择内容…</option>
                {contents.map((c) => (
                  <option key={c.id} value={c.content_no}>
                    {c.content_no} {c.title}
                  </option>
                ))}
              </select>
              <button className="primary" disabled={busy} onClick={() => reassignManual(r.submission_id)}>
                改派并识别
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function baseName(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}
