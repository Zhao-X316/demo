import { useEffect, useState } from "react";
import { TaskCard, TodayView, dashboardToday, dayRollover, humanDecide } from "../api/dashboard";
import { Class, RecContent, Student, TaskGenerateResult, classesList, contentsList, studentsList } from "../api/manage";
import { AudioPlayer } from "../components/AudioPlayer";
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

  if (!view) {
    return <div className="page">{err ? <div className="error">{err}</div> : <div className="loading">加载今日任务…</div>}</div>;
  }

  const s = view.summary;
  const confirmOne = async (t: TaskCard, result: "pass" | "fail" | "reopen", note?: string) => {
    if (!t.submission) return;
    setReviewBusy(true);
    setErr("");
    try {
      await humanDecide(t.submission.submission_id, result, note);
      setToast(result === "pass" ? "已确认通过" : result === "fail" ? "已确认不通过" : "已重开任务");
      await load();
    } catch (e) {
      setErr(String(e));
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
    if (filter === "wait") return t.submission?.pending_review === true;
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
          <button className="primary" onClick={() => setAssign(true)}>
            ＋ 布置背诵
          </button>
        </div>
      </div>
      <div className="sub">
        今日 {s.should} 项任务
        {view.overdue_review.length > 0 ? ` · 另有 ${view.overdue_review.length} 项逾期待终审` : ""}
        {" · "}机器结果仅供参考 · 请逐条查看录音和文本证据后终审
      </div>
      {err && <div className="error">{err}</div>}
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
      {(filter === "all" || filter === "wait") && (
        <Section title="逾期待老师处理" list={view.overdue_review} />
      )}

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
          onTasksChanged={(message) => {
            setToast(message);
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
  onDecide: (t: TaskCard, result: "pass" | "fail" | "reopen", note?: string) => void;
}) {
  const sub = t.submission;
  const [expanded, setExpanded] = useState(false);
  const [note, setNote] = useState(sub?.human_note ?? "");
  const [audioState, setAudioState] = useState<"idle" | "ready" | "error">("idle");
  const [rejudging, setRejudging] = useState(false);
  useEffect(() => {
    setNote(sub?.human_note ?? "");
    setAudioState("idle");
    setRejudging(false);
  }, [sub?.submission_id, sub?.file_path, sub?.human_note, sub?.human_result]);

  const dot =
    t.status === "passed" ? "" : t.status === "failed" ? "f" : sub ? "w" : "n";
  const hasAsr = Boolean(sub?.recognized_text?.trim());
  const hasAnswer = Boolean(sub?.answer_text?.trim());
  const versionMatches = Boolean(
    sub &&
    sub.answer_version !== null &&
    sub.scored_answer_version !== null &&
    sub.answer_version === sub.scored_answer_version,
  );
  const canDecide = audioState === "ready" && hasAsr && hasAnswer && versionMatches;
  const machineResult = sub?.pass ? "通过" : "不通过";
  const canReview = Boolean(sub && (sub.pending_review || sub.human_result));
  return (
    <div className="task-block">
      <div className="row">
        <span className={"dotg " + dot} />
        <Avatar name={t.student_name} />
        <div>
          <div className="who">
            {t.student_name} <span className="cls">{t.student_no}</span>
          </div>
          <div className="meta">
            {t.content_no} · {t.content_title}
            {t.due_date && <span> · {t.due_date}</span>}
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
        {sub?.pending_review && <span className="tag wait">待确认</span>}
        {sub && !sub.pending_review && !sub.human_result && sub.recognize_status === "failed" && (
          <span className="tag fail">识别失败 · 请到批改台处理</span>
        )}
        {sub && !sub.pending_review && !sub.human_result && sub.recognize_status !== "failed" && (
          <span className="tag">{sub.recognize_status === "processing" ? "识别中" : "待评分"}</span>
        )}
        {sub?.pending_review && sub.pass !== null && (
          <>
            <span className={"acc " + (sub.pass ? "p" : "f") + " num"}>{sub.accuracy ?? ""}</span>
            <span className={"tag " + (sub.pass ? "pass" : "fail")}>
              系统建议{machineResult}{sub.quality ? ` · ${sub.quality}` : ""}
            </span>
          </>
        )}
        {!sub && <span className="tag">未交</span>}
        {sub?.human_result && (
          <span className="tag">
            老师{sub.human_result === "pass" ? "确认通过" : sub.human_result === "fail" ? "确认不通过" : "已重开"}
          </span>
        )}
        {canReview && (
          <button className="sm" onClick={() => setExpanded(!expanded)}>
            {expanded ? "收起证据" : sub?.human_result ? "查看终审记录" : "查看证据并终审"}
          </button>
        )}
      </div>

      {expanded && sub && (
        <div className="evidence-panel">
          <div className="evidence-head">
            <div>
              <b>终审证据</b>
              <span>提交 #{sub.submission_id}</span>
            </div>
            <span className={"tag " + (sub.pass ? "pass" : "fail")}>机器建议{machineResult}</span>
          </div>

          <div className="evidence-grid">
            <section>
              <h3>原始录音</h3>
              <AudioPlayer
                path={sub.file_path}
                onReady={() => setAudioState("ready")}
                onError={() => setAudioState("error")}
              />
              {audioState === "idle" && <div className="evidence-hint">正在验证录音是否可读…</div>}
              {audioState === "error" && <div className="evidence-error">录音无法读取或格式不受支持，不能终审；可重开任务后重新提交。</div>}
            </section>
            <section>
              <h3>评分信息</h3>
              <div className="evidence-metrics">
                <span>正确率 <b>{sub.accuracy ?? "—"}</b></span>
                <span>熟练度 <b>{sub.fluency ?? "—"}</b></span>
                <span>等级 <b>{sub.quality ?? "—"}</b></span>
              </div>
              <div className="machine-note">{sub.machine_note || "系统未提供评分说明"}</div>
            </section>
            <section>
              <h3>ASR 原文</h3>
              <div className={hasAsr ? "evidence-text" : "evidence-text missing"}>
                {hasAsr ? sub.recognized_text : "ASR 原文为空，不能终审"}
              </div>
            </section>
            <section>
              <h3>
                标准答案
                <span className="version">当前 v{sub.answer_version ?? "—"} · 评分 v{sub.scored_answer_version ?? "—"}</span>
              </h3>
              <div className={hasAnswer ? "evidence-text" : "evidence-text missing"}>
                {hasAnswer ? sub.answer_text : "标准答案为空，不能终审"}
              </div>
              {!versionMatches && <div className="evidence-error">答案版本已变化，请先重新分析，再进行终审。</div>}
            </section>
          </div>

          <label className="evidence-note">
            <span>人工备注</span>
            <textarea
              rows={3}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              placeholder="记录听辨依据、错漏位置或改判原因（可选）"
              disabled={Boolean(sub.human_result) && !rejudging}
            />
          </label>

          {sub.human_result ? (
            rejudging ? (
              <div className="evidence-actions">
                <div className="evidence-hint">
                  改判会先回滚旧副作用，再应用新结论；状态已漂移时系统会拒绝自动覆盖。
                </div>
                <div className="spacer" />
                <button disabled={busy} onClick={() => setRejudging(false)}>取消</button>
                <button
                  className="primary"
                  disabled={busy || !canDecide}
                  onClick={() => onDecide(t, sub.human_result === "pass" ? "fail" : "pass", note)}
                >
                  确认改判为{sub.human_result === "pass" ? "不通过" : "通过"}
                </button>
              </div>
            ) : (
              <div className="reviewed-result">
                <span>
                  已终审：{sub.human_result === "pass" ? "通过" : sub.human_result === "fail" ? "不通过" : "重开"}
                  {sub.human_note ? ` · ${sub.human_note}` : ""}
                </span>
                {sub.human_result !== "reopen" && (
                  <button disabled={busy} onClick={() => setRejudging(true)}>发起改判</button>
                )}
              </div>
            )
          ) : (
            <div className="evidence-actions">
              <div className="evidence-hint">
                {canDecide ? "证据已就绪，可以终审。" : "需录音可读、ASR/答案非空且答案版本一致后才能确认。"}
              </div>
              <div className="spacer" />
              <button disabled={busy} onClick={() => onDecide(t, "reopen", note)}>重开待重交</button>
              <button disabled={busy || !canDecide} onClick={() => onDecide(t, sub.pass ? "fail" : "pass", note)}>
                判为{sub.pass ? "不通过" : "通过"}
              </button>
              <button className="primary" disabled={busy || !canDecide} onClick={() => onDecide(t, sub.pass ? "pass" : "fail", note)}>
                确认系统建议
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
