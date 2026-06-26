import { useCallback, useEffect, useMemo, useState } from "react";
import {
  dashboardToday,
  dayRollover,
  humanDecide,
  seedDemo,
  type TaskCard,
  type TodaySummary,
  type TodayView,
} from "../api/dashboard";
import { AudioPlayer } from "../components/AudioPlayer";

type Metric = "all" | "submitted" | "passed" | "failed" | "pending";
interface Filter {
  metric: Metric;
  contentNo: string | null;
  kind: "all" | "makeup"; // 补背维度（与 metric 正交）
}

const METRIC_LABEL: Record<Metric, string> = {
  all: "全部",
  submitted: "实背",
  passed: "通过",
  failed: "不通过",
  pending: "待确认",
};

function cardMatches(c: TaskCard, f: Filter): boolean {
  if (f.contentNo && c.content_no !== f.contentNo) return false;
  if (f.kind === "makeup" && c.kind !== "makeup") return false;
  switch (f.metric) {
    case "all":
      return true;
    case "submitted":
      return c.submission != null;
    case "passed":
      return c.status === "passed";
    case "failed":
      return c.status === "failed";
    case "pending":
      return c.submission != null && c.status !== "passed" && c.status !== "failed";
  }
}

export default function Dashboard() {
  const [view, setView] = useState<TodayView | null>(null);
  const [err, setErr] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState<Filter>({ metric: "all", contentNo: null, kind: "all" });

  const refresh = useCallback(async () => {
    try {
      setView(await dashboardToday());
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    let day = new Date().toDateString();
    const boot = async () => {
      // 加载前先跑一次日切（启动钩子已兜底，这里再补一次确保看板最新）
      try {
        await dayRollover();
      } catch {
        /* 忽略：启动钩子已处理 */
      }
      await refresh();
    };
    void boot();
    // 常驻兜底：每 5 分钟检查是否跨天，跨天则重跑日切 + 刷新（App 一直开着也不漏补背/复习）
    const timer = window.setInterval(() => {
      const now = new Date().toDateString();
      if (now !== day) {
        day = now;
        void boot();
      }
    }, 5 * 60 * 1000);
    return () => window.clearInterval(timer);
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

  const sum = view?.summary;
  const total = view ? view.normal.length + view.makeup.length + view.review.length : 0;
  const filtered = useMemo(() => {
    if (!view) return { normal: [], makeup: [], review: [] };
    return {
      normal: view.normal.filter((c) => cardMatches(c, filter)),
      makeup: view.makeup.filter((c) => cardMatches(c, filter)),
      review: view.review.filter((c) => cardMatches(c, filter)),
    };
  }, [view, filter]);

  const filterActive = filter.metric !== "all" || filter.contentNo != null || filter.kind !== "all";
  const toggleMetric = (m: Metric) =>
    setFilter((f) => ({ ...f, metric: f.metric === m ? "all" : m }));
  const toggleContent = (no: string) =>
    setFilter((f) => ({ ...f, contentNo: f.contentNo === no ? null : no }));
  const toggleMakeup = () =>
    setFilter((f) => ({ ...f, kind: f.kind === "makeup" ? "all" : "makeup" }));
  const clearFilter = () => setFilter({ metric: "all", contentNo: null, kind: "all" });

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

      {sum && total > 0 && (
        <>
          <SummaryBar
            sum={sum}
            metric={filter.metric}
            makeupActive={filter.kind === "makeup"}
            onPick={toggleMetric}
            onPickMakeup={toggleMakeup}
          />
          <ContentTable
            sum={sum}
            activeNo={filter.contentNo}
            onPick={toggleContent}
          />
          {filterActive && (
            <div className="filter-bar">
              筛选：
              {filter.metric !== "all" && <b>{METRIC_LABEL[filter.metric]}</b>}
              {filter.kind === "makeup" && <b> · 补背</b>}
              {filter.contentNo && <b> · 内容 {filter.contentNo}</b>}
              <button className="link" onClick={clearFilter}>
                清除筛选
              </button>
            </div>
          )}
        </>
      )}

      <Group title="新背" cards={filtered.normal} onDecide={onDecide} busy={busy} />
      <Group title="补背" cards={filtered.makeup} onDecide={onDecide} busy={busy} />
      <Group title="到期复习" cards={filtered.review} onDecide={onDecide} busy={busy} />
      {filterActive &&
        filtered.normal.length + filtered.makeup.length + filtered.review.length === 0 && (
          <p className="muted">该筛选下没有学生。</p>
        )}
    </div>
  );
}

function SummaryBar(props: {
  sum: TodaySummary;
  metric: Metric;
  makeupActive: boolean;
  onPick: (m: Metric) => void;
  onPickMakeup: () => void;
}) {
  const { sum, metric, makeupActive, onPick, onPickMakeup } = props;
  const stats: { key: Metric; label: string; value: number; tone?: string }[] = [
    { key: "all", label: "应背人数", value: sum.should },
    { key: "submitted", label: "实背人数", value: sum.submitted },
    { key: "passed", label: "通过人数", value: sum.passed, tone: "ok" },
    { key: "failed", label: "不通过人数", value: sum.failed, tone: "bad" },
    { key: "pending", label: "待确认", value: sum.pending },
  ];
  return (
    <div className="stat-row">
      {stats.map((s) => (
        <button
          key={s.key}
          className={`stat-card ${s.tone ?? ""} ${metric === s.key ? "active" : ""}`}
          onClick={() => onPick(s.key)}
          title="点击筛选下方学生"
        >
          <div className="stat-value">{s.value}</div>
          <div className="stat-label">{s.label}</div>
        </button>
      ))}
      <button
        className={`stat-card ${makeupActive ? "active" : ""}`}
        onClick={onPickMakeup}
        title="点击只看补背学生"
      >
        <div className="stat-value">{sum.makeup}</div>
        <div className="stat-label">补背人数</div>
      </button>
    </div>
  );
}

function ContentTable(props: {
  sum: TodaySummary;
  activeNo: string | null;
  onPick: (no: string) => void;
}) {
  const { sum, activeNo, onPick } = props;
  if (sum.contents.length === 0) return null;
  return (
    <section className="group">
      <h2>今日背诵内容 <span className="badge">{sum.contents.length}</span></h2>
      <table className="tbl">
        <thead>
          <tr>
            <th>内容</th>
            <th>应背</th>
            <th>实背</th>
            <th>通过</th>
            <th>不通过</th>
          </tr>
        </thead>
        <tbody>
          {sum.contents.map((c) => (
            <tr
              key={c.content_no}
              className={`clickable ${activeNo === c.content_no ? "active-row" : ""}`}
              onClick={() => onPick(c.content_no)}
              title="点击只看这个内容的学生"
            >
              <td>
                {c.content_no} · {c.content_title}
              </td>
              <td>{c.should}</td>
              <td>{c.submitted}</td>
              <td className="ok">{c.passed}</td>
              <td className="bad">{c.failed}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
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
