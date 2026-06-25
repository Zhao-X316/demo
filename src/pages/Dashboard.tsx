import { useCallback, useEffect, useState } from "react";
import {
  dashboardToday,
  humanDecide,
  seedDemo,
  type TaskCard,
  type TodayView,
} from "../api/dashboard";
import { AudioPlayer } from "../components/AudioPlayer";

export default function Dashboard() {
  const [view, setView] = useState<TodayView | null>(null);
  const [err, setErr] = useState<string>("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setView(await dashboardToday());
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onSeed = async () => {
    setBusy(true);
    try {
      await seedDemo();
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onDecide = async (id: number, result: string) => {
    setBusy(true);
    try {
      await humanDecide(id, result);
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const total = view ? view.normal.length + view.makeup.length + view.review.length : 0;

  return (
    <div className="page">
      <div className="page-head">
        <h1>
          今日看板 <span className="muted">{view?.date ?? ""}</span>
        </h1>
        <button className="primary" onClick={onSeed} disabled={busy}>
          生成示例数据
        </button>
      </div>

      {err && <div className="error">出错：{err}</div>}
      {total === 0 && !err && (
        <p className="muted">今天没有任务。点"生成示例数据"即可体验完整流程（无需录音/ASR）。</p>
      )}

      <Group title="新背" cards={view?.normal ?? []} onDecide={onDecide} busy={busy} />
      <Group title="补背" cards={view?.makeup ?? []} onDecide={onDecide} busy={busy} />
      <Group title="到期复习" cards={view?.review ?? []} onDecide={onDecide} busy={busy} />
    </div>
  );
}

function Group(props: {
  title: string;
  cards: TaskCard[];
  onDecide: (id: number, r: string) => void;
  busy: boolean;
}) {
  const { title, cards, onDecide, busy } = props;
  if (cards.length === 0) return null;
  return (
    <section className="group">
      <h2>
        {title} <span className="badge">{cards.length}</span>
      </h2>
      <div className="cards">
        {cards.map((c) => (
          <Card key={c.task_id} c={c} onDecide={onDecide} busy={busy} />
        ))}
      </div>
    </section>
  );
}

function Card(props: { c: TaskCard; onDecide: (id: number, r: string) => void; busy: boolean }) {
  const { c, onDecide, busy } = props;
  const s = c.submission;
  return (
    <div className="card">
      <div className="card-top">
        <strong>{c.student_name}</strong>
        <span className="muted">{c.student_no}</span>
        <span className={`pill pill-${c.status}`}>{statusLabel(c.status)}</span>
      </div>
      <div className="muted">
        {c.content_no} · {c.content_title}
      </div>
      {s ? (
        <div className="verdict">
          <div className="scores">
            <span className={`acc ${s.pass ? "ok" : "bad"}`}>
              正确率 {fmt(s.accuracy)}%
              {s.pass != null && (s.pass ? " ✓通过" : " ✗未达标")}
            </span>
            <span className="flu">
              熟练度 {fmt(s.fluency)} {s.quality ? `(${s.quality})` : ""}
            </span>
          </div>
          <AudioPlayer path={s.file_path} />
          {s.recognized_text && <div className="asr">识别：{s.recognized_text}</div>}
          {s.machine_note && <div className="note muted">{s.machine_note}</div>}
          <div className="actions">
            <button disabled={busy} onClick={() => onDecide(s.submission_id, "pass")}>
              通过
            </button>
            <button disabled={busy} onClick={() => onDecide(s.submission_id, "fail")}>
              未通过
            </button>
            <button disabled={busy} onClick={() => onDecide(s.submission_id, "reopen")}>
              打回
            </button>
          </div>
          {s.human_result && <div className="muted">人工结论：{s.human_result}</div>}
        </div>
      ) : (
        <div className="muted">未提交</div>
      )}
    </div>
  );
}

function fmt(n: number | null): string {
  return n == null ? "—" : Math.round(n).toString();
}

function statusLabel(s: string): string {
  const m: Record<string, string> = {
    open: "待交",
    submitted: "已交",
    passed: "通过",
    failed: "未通过",
    reopened: "打回",
    closed: "关闭",
    expired: "过期",
  };
  return m[s] ?? s;
}
