import { useEffect, useState } from "react";
import { TaskCard, TodayView, dashboardToday, dayRollover, humanDecide } from "../api/dashboard";
import { Class, RecContent, Student, TaskGenerateResult, classesList, contentsList, studentsList } from "../api/manage";
import { Avatar } from "../components/ui";
import { AssignModal } from "../components/modals";

type Filter = "all" | "sub" | "pass" | "wait" | "makeup";

function taskResultText(r: TaskGenerateResult) {
  const parts = [`新建 ${r.created}`];
  if (r.revived) parts.push(`恢复 ${r.revived}`);
  if (r.skipped_open) parts.push(`跳过未完成 ${r.skipped_open}`);
  if (r.skipped_completed) parts.push(`跳过已完成 ${r.skipped_completed}`);
  return parts.join("，");
}

export default function Today() {
  const [view, setView] = useState<TodayView | null>(null);
  const [err, setErr] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [assign, setAssign] = useState(false);
  const [students, setStudents] = useState<Student[]>([]);
  const [contents, setContents] = useState<RecContent[]>([]);
  const [classes, setClasses] = useState<Class[]>([]);
  const [toast, setToast] = useState("");
  const [reviewBusy, setReviewBusy] = useState(false);

  const load = async () => {
    try {
      await dayRollover().catch(() => undefined); // 日切：没交→补背 / 到期→复习
      setView(await dashboardToday());
    } catch (e) {
      setErr(String(e));
    }
  };
  useEffect(() => {
    load();
    studentsList().then(setStudents).catch(() => undefined);
    contentsList().then(setContents).catch(() => undefined);
    classesList().then(setClasses).catch(() => undefined);
  }, []);

  if (err) return <div className="page"><div className="error">{err}</div></div>;
  if (!view) return <div className="page"><div className="loading">加载今日任务…</div></div>;

  const s = view.summary;
  const confirmable = [...view.normal, ...view.makeup, ...view.review].filter(
    (t) =>
      t.submission &&
      !t.submission.human_result &&
      (t.status === "passed" || t.status === "failed"),
  );
  const confirmOne = async (t: TaskCard, result?: "pass" | "fail" | "reopen") => {
    if (!t.submission) return;
    setReviewBusy(true);
    setErr("");
    try {
      const r = result ?? (t.status === "passed" ? "pass" : "fail");
      await humanDecide(t.submission.submission_id, r);
      setToast(r === "pass" ? "已确认通过" : r === "fail" ? "已确认不通过" : "已重开任务");
      await load();
    } catch (e) {
      setErr(String(e));
    }
    setReviewBusy(false);
  };
  const confirmAll = async () => {
    setReviewBusy(true);
    setErr("");
    let ok = 0;
    try {
      for (const t of confirmable) {
        if (!t.submission) continue;
        await humanDecide(t.submission.submission_id, t.status === "passed" ? "pass" : "fail");
        ok += 1;
      }
      setToast(`已一键确认 ${ok} 条系统判定`);
      await load();
    } catch (e) {
      setErr(`已确认 ${ok} 条，后续失败：${String(e)}`);
    }
    setReviewBusy(false);
  };
  const stats: { k: Filter; v: number; l: string; c?: string }[] = [
    { k: "all", v: s.should, l: "今日应背" },
    { k: "sub", v: s.submitted, l: "已交" },
    { k: "pass", v: s.passed, l: "通过", c: "ok" },
    { k: "wait", v: s.pending, l: "待确认" },
    { k: "makeup", v: s.makeup, l: "补背", c: "bad" },
  ];
  const match = (t: TaskCard) => {
    if (filter === "all") return true;
    if (filter === "sub") return t.submission != null;
    if (filter === "pass") return t.status === "passed";
    if (filter === "wait") return t.submission != null && t.status === "submitted";
    if (filter === "makeup") return t.kind === "makeup";
    return true;
  };

  const Section = ({ title, list }: { title: string; list: TaskCard[] }) => {
    const f = list.filter(match);
    if (!f.length) return null;
    return (
      <div>
        <div className="sech">
          {title}
          <span className="n">{f.length} 人</span>
        </div>
        {f.map((t) => (
          <TaskRow key={t.task_id} t={t} busy={reviewBusy} onDecide={confirmOne} />
        ))}
      </div>
    );
  };

  return (
    <div className="page">
      <div className="page-head">
        <h1>今日</h1>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <span className="date">{view.date}</span>
          {confirmable.length > 0 && (
            <button disabled={reviewBusy} onClick={confirmAll}>
              {reviewBusy ? "确认中…" : `一键确认 ${confirmable.length}`}
            </button>
          )}
          <button className="primary" onClick={() => setAssign(true)}>
            ＋ 布置背诵
          </button>
        </div>
      </div>
      <div className="sub">
        共 {s.should} 项任务 · 待确认会在「待确认」高亮 · 系统判定后由你终审
      </div>
      {toast && <div className="ok-banner">{toast}</div>}
      <div className="stats">
        {stats.map((st) => (
          <button
            key={st.k}
            className={"stat " + (st.c ?? "") + (filter === st.k ? " on" : "")}
            onClick={() => setFilter(filter === st.k ? "all" : st.k)}
          >
            <div className="v num">{st.v}</div>
            <div className="l">{st.l}</div>
          </button>
        ))}
      </div>
      {filter !== "all" && (
        <div className="filterbar">
          筛选：<b>{stats.find((x) => x.k === filter)!.l}</b> ·{" "}
          <button className="link" onClick={() => setFilter("all")}>
            清除
          </button>
        </div>
      )}
      <Section title="新背" list={view.normal} />
      <Section title="补背" list={view.makeup} />
      <Section title="复习" list={view.review} />

      {assign && (
        <AssignModal
          students={students}
          contents={contents}
          classList={classes}
          onClose={() => setAssign(false)}
          onDone={(result) => {
            setAssign(false);
            setToast(`已布置：${taskResultText(result)}`);
            load();
          }}
        />
      )}
    </div>
  );
}

function TaskRow({
  t,
  busy,
  onDecide,
}: {
  t: TaskCard;
  busy: boolean;
  onDecide: (t: TaskCard, result?: "pass" | "fail" | "reopen") => void;
}) {
  const sub = t.submission;
  const dot =
    t.status === "passed" ? "" : t.status === "failed" ? "f" : sub ? "w" : "n";
  return (
    <div className="row">
      <span className={"dotg " + dot} />
      <Avatar name={t.student_name} />
      <div>
        <div className="who">
          {t.student_name} <span className="cls">{t.student_no}</span>
        </div>
        <div className="meta">
          {t.content_no} · {t.content_title}
        </div>
      </div>
      <div className="spacer" />
      {t.status === "passed" && sub && (
        <>
          <span className="acc p num">{sub.accuracy ?? ""}</span>
          <span className="tag pass">通过{sub.quality ? ` · ${sub.quality}` : ""}</span>
        </>
      )}
      {t.status === "failed" && sub && (
        <>
          <span className="acc f num">{sub.accuracy ?? ""}</span>
          <span className="tag fail">不通过</span>
          <span className="tag makeup">排补背</span>
        </>
      )}
      {t.status === "submitted" && sub && <span className="tag wait">待确认</span>}
      {!sub && <span className="tag">未交</span>}
      {sub?.human_result && (
        <span className="tag">
          老师{sub.human_result === "pass" ? "确认通过" : sub.human_result === "fail" ? "确认不通过" : "已重开"}
        </span>
      )}
      {sub && !sub.human_result && (t.status === "passed" || t.status === "failed") && (
        <div style={{ display: "flex", gap: 6 }}>
          <button className="sm" disabled={busy} onClick={() => onDecide(t)}>
            确认
          </button>
          <button className="sm" disabled={busy} onClick={() => onDecide(t, t.status === "passed" ? "fail" : "pass")}>
            改判{t.status === "passed" ? "不通过" : "通过"}
          </button>
          <button className="sm" disabled={busy} onClick={() => onDecide(t, "reopen")}>
            重开
          </button>
        </div>
      )}
    </div>
  );
}
