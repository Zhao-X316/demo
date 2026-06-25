import { useCallback, useEffect, useState } from "react";
import { anomaliesList, anomalyReassign, type Anomaly } from "../api/anomaly";
import { asrAndScore } from "../api/importing";
import { AudioPlayer } from "../components/AudioPlayer";

export default function AnomalyPage() {
  const [rows, setRows] = useState<Anomaly[]>([]);
  const [edit, setEdit] = useState<Record<number, { sno: string; cno: string }>>({});
  const [err, setErr] = useState("");
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setRows(await anomaliesList());
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

  const field = (id: number, k: "sno" | "cno", v: string) =>
    setEdit((s) => ({
      ...s,
      [id]: { sno: s[id]?.sno ?? "", cno: s[id]?.cno ?? "", [k]: v },
    }));

  // 改派后立即识别评分（同一 submission_id）
  const reassignAndScore = async (id: number) => {
    const f = edit[id] ?? { sno: "", cno: "" };
    setErr("");
    setBusy(true);
    try {
      await anomalyReassign(id, f.sno || undefined, f.cno || undefined);
    } catch (e) {
      setErr("改派失败：" + String(e));
      setBusy(false);
      return;
    }
    try {
      const o = await asrAndScore(id);
      flash(`已改派并识别：正确率 ${Math.round(o.accuracy)}% ${o.pass ? "通过" : "未达标"}`);
    } catch (e) {
      flash("已改派；识别评分待办（" + String(e) + "）");
    }
    await refresh();
    setBusy(false);
  };

  return (
    <div className="page">
      <div className="page-head">
        <h1>异常池 <span className="badge">{rows.length}</span></h1>
        <button onClick={() => void refresh()} disabled={busy}>刷新</button>
      </div>
      <p className="muted">填入正确的学号 / 内容编号（任一可留空，沿用已解析值），点「改派并识别」修复。</p>
      {err && <div className="error">出错：{err}</div>}
      {msg && <div className="ok-banner">{msg}</div>}
      {rows.length === 0 && !err && <p className="muted">没有异常项 🎉</p>}

      {rows.map((r) => (
        <div className="card" key={r.submission_id}>
          <div className="card-top">
            <span className="pill pill-failed">{r.anomaly_type}</span>
            <span className="muted">{baseName(r.file_path)}</span>
          </div>
          {r.parsed_meta && <div className="muted">解析：{r.parsed_meta}</div>}
          <AudioPlayer path={r.file_path} />
          <div className="row" style={{ marginTop: 10 }}>
            <input
              placeholder="正确学号"
              value={edit[r.submission_id]?.sno ?? ""}
              onChange={(ev) => field(r.submission_id, "sno", ev.target.value)}
            />
            <input
              placeholder="正确内容编号"
              value={edit[r.submission_id]?.cno ?? ""}
              onChange={(ev) => field(r.submission_id, "cno", ev.target.value)}
            />
            <button className="primary" disabled={busy} onClick={() => reassignAndScore(r.submission_id)}>
              改派并识别
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

function baseName(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}
