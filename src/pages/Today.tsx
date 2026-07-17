import { useEffect, useRef, useState } from "react";
import {
  RecitationPointState,
  StructuredPointCard,
  TaskCard,
  TeacherPointReviewInput,
  TodayView,
  dashboardToday,
  dayRollover,
  humanDecide,
} from "../api/dashboard";
import { Class, RecContent, Student, TaskGenerateResult, classesList, contentsList, studentsList } from "../api/manage";
import { AudioPlayer, AudioPlayerHandle } from "../components/AudioPlayer";
import { Avatar } from "../components/ui";
import { AssignModal } from "../components/modals";

type Filter = "all" | "sub" | "pass" | "wait" | "makeup";
type ReviewMode = "view" | "rejudge" | "points";
type PointDraft = {
  confirmation: "accepted" | "corrected";
  state: RecitationPointState;
  note: string;
};
type ReviewRisk = {
  rank: number;
  label: string;
  detail: string;
  tone: "blocking" | "review" | "normal";
  confidence: number;
};

const POINT_STATE_LABEL: Record<RecitationPointState, string> = {
  covered: "已覆盖",
  partial: "部分覆盖",
  omitted: "遗漏",
  contradiction: "存在矛盾",
  uncertain: "无法确定",
};

function pointDraft(point: StructuredPointCard): PointDraft {
  return {
    confirmation: point.teacher_confirmation_level ?? "accepted",
    state: point.teacher_state ?? point.machine_state,
    note: point.teacher_note ?? "",
  };
}

function formatTimestamp(milliseconds: number) {
  const totalTenths = Math.max(0, Math.floor(milliseconds / 100));
  const minutes = Math.floor(totalTenths / 600);
  const seconds = (totalTenths % 600) / 10;
  return `${minutes}:${seconds.toFixed(1).padStart(4, "0")}`;
}

function validEvidenceSpans(point: StructuredPointCard) {
  return point.evidence_spans
    .map((span, index) => ({ ...span, index }))
    .filter(
      (span): span is typeof span & { start_ms: number; end_ms: number } =>
        typeof span.start_ms === "number" &&
        Number.isFinite(span.start_ms) &&
        span.start_ms >= 0 &&
        typeof span.end_ms === "number" &&
        Number.isFinite(span.end_ms) &&
        span.end_ms > span.start_ms,
    );
}

function reviewRisk(task: TaskCard): ReviewRisk | null {
  const sub = task.submission;
  if (!sub?.pending_review) return null;
  const evidenceIncomplete =
    !sub.recognized_text?.trim() ||
    !sub.answer_text?.trim() ||
    sub.answer_version === null ||
    sub.scored_answer_version === null ||
    sub.answer_version !== sub.scored_answer_version;
  if (evidenceIncomplete) {
    return {
      rank: 0,
      label: "证据异常",
      detail: "ASR、答案或版本证据不完整",
      tone: "blocking",
      confidence: 0,
    };
  }
  const score = sub.structured_score;
  const states = score?.points.map((point) => point.machine_state) ?? [];
  if (states.includes("contradiction")) {
    return {
      rank: 1,
      label: "事实矛盾",
      detail: "评分点命中矛盾表述",
      tone: "blocking",
      confidence: score?.confidence ?? 0,
    };
  }
  if (score?.overall_suggestion === "unable_to_score" || states.includes("uncertain")) {
    return {
      rank: 2,
      label: "无法确定",
      detail: "机器证据不确定，需要优先听辨",
      tone: "review",
      confidence: score?.confidence ?? 0,
    };
  }
  if (sub.pass === false || states.includes("omitted") || states.includes("partial")) {
    return {
      rank: 3,
      label: "错漏待核对",
      detail: "机器建议不通过或发现评分点错漏",
      tone: "review",
      confidence: score?.confidence ?? 0,
    };
  }
  return {
    rank: 4,
    label: "通过待核对",
    detail: "机器建议通过，仍需老师确认",
    tone: "normal",
    confidence: score?.confidence ?? 1,
  };
}

function pendingReviewQueue(view: TodayView) {
  const unique = new Map<number, TaskCard>();
  [...view.normal, ...view.makeup, ...view.review, ...view.overdue_review].forEach((task) => {
    if (task.submission?.pending_review) unique.set(task.task_id, task);
  });
  return [...unique.values()].sort((left, right) => {
    const leftRisk = reviewRisk(left)!;
    const rightRisk = reviewRisk(right)!;
    return (
      leftRisk.rank - rightRisk.rank ||
      leftRisk.confidence - rightRisk.confidence ||
      left.student_no.localeCompare(right.student_no, "zh-CN", { numeric: true })
    );
  });
}

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
  const [expandedTaskId, setExpandedTaskId] = useState<number | null>(null);

  const load = async (): Promise<TodayView | null> => {
    try {
      await dayRollover().catch(() => undefined); // 日切：没交→补背 / 到期→复习
      const nextView = await dashboardToday();
      setView(nextView);
      return nextView;
    } catch (e) {
      setErr(String(e));
      return null;
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
  const confirmOne = async (
    t: TaskCard,
    result: "pass" | "fail" | "reopen",
    note?: string,
    pointReview?: TeacherPointReviewInput,
  ) => {
    if (!t.submission) return;
    setReviewBusy(true);
    setErr("");
    try {
      await humanDecide(t.submission.submission_id, result, note, pointReview);
      setToast(result === "pass" ? "已确认通过" : result === "fail" ? "已确认不通过" : "已重开任务");
      const nextView = await load();
      if (!t.submission.human_result) {
        setExpandedTaskId(nextView ? pendingReviewQueue(nextView)[0]?.task_id ?? null : null);
      }
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
          <TaskRow
            key={t.task_id}
            t={t}
            busy={reviewBusy}
            expanded={expandedTaskId === t.task_id}
            onToggle={() => setExpandedTaskId(expandedTaskId === t.task_id ? null : t.task_id)}
            onDecide={confirmOne}
          />
        ))}
      </div>
    );
  };
  const pendingQueue = pendingReviewQueue(view);
  const withoutPending = (list: TaskCard[]) =>
    list.filter((task) => !task.submission?.pending_review);

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
      <Section title="待终审 · 按风险排序" list={pendingQueue} />
      <Section title="新背" list={withoutPending(view.normal)} />
      <Section title="补背" list={withoutPending(view.makeup)} />
      <Section title="复习" list={withoutPending(view.review)} />
      {(filter === "all" || filter === "wait") && (
        <Section title="逾期待老师处理" list={withoutPending(view.overdue_review)} />
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
  expanded,
  onToggle,
  onDecide,
}: {
  t: TaskCard;
  busy: boolean;
  expanded: boolean;
  onToggle: () => void;
  onDecide: (
    t: TaskCard,
    result: "pass" | "fail" | "reopen",
    note?: string,
    pointReview?: TeacherPointReviewInput,
  ) => void;
}) {
  const sub = t.submission;
  const [note, setNote] = useState(sub?.human_note ?? "");
  const [audioState, setAudioState] = useState<"idle" | "ready" | "error">("idle");
  const [reviewMode, setReviewMode] = useState<ReviewMode>("view");
  const [pointReviewEnabled, setPointReviewEnabled] = useState(false);
  const [pointDrafts, setPointDrafts] = useState<Record<number, PointDraft>>({});
  const [pointError, setPointError] = useState("");
  const [activeSpanKey, setActiveSpanKey] = useState("");
  const [spanCursor, setSpanCursor] = useState(-1);
  const audioPlayerRef = useRef<AudioPlayerHandle>(null);
  const noteRef = useRef<HTMLTextAreaElement>(null);
  const blockRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setNote(sub?.human_note ?? "");
    setAudioState("idle");
    setReviewMode("view");
    setPointReviewEnabled(Boolean(sub?.structured_score?.review_revision));
    setPointDrafts(
      Object.fromEntries(
        (sub?.structured_score?.points ?? []).map((point) => [point.point_result_id, pointDraft(point)]),
      ),
    );
    setPointError("");
    setActiveSpanKey("");
    setSpanCursor(-1);
  }, [
    sub?.submission_id,
    sub?.file_path,
    sub?.human_note,
    sub?.human_result,
    sub?.structured_score?.score_run_id,
    sub?.structured_score?.review_revision,
  ]);

  const togglePointReview = (enabled: boolean) => {
    setPointReviewEnabled(enabled);
    setPointError("");
    if (enabled) {
      setPointDrafts(
        Object.fromEntries(
          (sub?.structured_score?.points ?? []).map((point) => [
            point.point_result_id,
            pointDrafts[point.point_result_id] ?? pointDraft(point),
          ]),
        ),
      );
    }
  };

  const buildPointReview = (): TeacherPointReviewInput | undefined => {
    const score = sub?.structured_score;
    if (!pointReviewEnabled || !score) return undefined;
    const items = score.points.map((point) => {
      const draft = pointDrafts[point.point_result_id] ?? pointDraft(point);
      if (draft.confirmation === "corrected" && !draft.note.trim()) {
        throw new Error(`“${point.canonical_text}”已人工修正，请填写简短说明`);
      }
      return {
        point_result_id: point.point_result_id,
        confirmation_level: draft.confirmation,
        corrected_state: draft.confirmation === "corrected" ? draft.state : null,
        corrected_evidence_spans_json:
          draft.confirmation === "corrected" ? JSON.stringify(point.evidence_spans) : null,
        teacher_note: draft.note.trim() || null,
      };
    });
    return { score_run_id: score.score_run_id, items };
  };

  const decide = (result: "pass" | "fail" | "reopen") => {
    setPointError("");
    try {
      onDecide(t, result, note, result === "reopen" ? undefined : buildPointReview());
    } catch (error) {
      setPointError(String(error));
    }
  };

  const beginRejudge = () => {
    setReviewMode("rejudge");
    setPointReviewEnabled(false);
    setPointDrafts(
      Object.fromEntries(
        (sub?.structured_score?.points ?? []).map((point) => [
          point.point_result_id,
          {
            confirmation: "accepted",
            state: point.machine_state,
            note: "",
          } satisfies PointDraft,
        ]),
      ),
    );
    setPointError("");
  };

  const beginPointEdit = () => {
    setReviewMode("points");
    setPointReviewEnabled(true);
    setPointDrafts(
      Object.fromEntries(
        (sub?.structured_score?.points ?? []).map((point) => [
          point.point_result_id,
          pointDraft(point),
        ]),
      ),
    );
    setPointError("");
  };

  const playEvidenceSpan = async (
    point: StructuredPointCard,
    span: { start_ms: number; end_ms: number; index: number },
  ) => {
    const key = `${point.point_result_id}:${span.index}`;
    setPointError("");
    try {
      if (audioState !== "ready" || !audioPlayerRef.current) {
        throw new Error("录音尚未准备好，请稍后再试");
      }
      await audioPlayerRef.current.playRange(span.start_ms, span.end_ms);
      setActiveSpanKey(key);
      const allSpans =
        sub?.structured_score?.points.flatMap((candidate) =>
          validEvidenceSpans(candidate).map((candidateSpan) => ({
            point: candidate,
            span: candidateSpan,
          })),
        ) ?? [];
      setSpanCursor(
        allSpans.findIndex(
          (candidate) =>
            candidate.point.point_result_id === point.point_result_id &&
            candidate.span.index === span.index,
        ),
      );
    } catch (error) {
      setActiveSpanKey("");
      setPointError(String(error));
    }
  };

  useEffect(() => {
    if (expanded) blockRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [expanded]);

  useEffect(() => {
    if (!expanded || !sub) return undefined;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) return;
      const target = event.target as HTMLElement | null;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement ||
        target?.isContentEditable
      ) {
        return;
      }
      const key = event.key.toLowerCase();
      if (key === "n") {
        event.preventDefault();
        noteRef.current?.focus();
        return;
      }
      if (key === " ") {
        event.preventDefault();
        audioPlayerRef.current?.togglePlayback().catch((error) => setPointError(String(error)));
        return;
      }
      if (key === "j" || key === "k") {
        const spans =
          sub.structured_score?.points.flatMap((point) =>
            validEvidenceSpans(point).map((span) => ({ point, span })),
          ) ?? [];
        if (!spans.length) return;
        event.preventDefault();
        const delta = key === "j" ? 1 : -1;
        const nextIndex =
          spanCursor < 0
            ? key === "j"
              ? 0
              : spans.length - 1
            : (spanCursor + delta + spans.length) % spans.length;
        const next = spans[nextIndex];
        playEvidenceSpan(next.point, next.span);
        return;
      }
      if (
        !sub.human_result &&
        !busy &&
        canDecide &&
        (key === "p" || key === "f")
      ) {
        event.preventDefault();
        decide(key === "p" ? "pass" : "fail");
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

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
  const risk = reviewRisk(t);
  return (
    <div className="task-block" ref={blockRef}>
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
        {risk && (
          <span className={`tag review-risk ${risk.tone}`} title={risk.detail}>
            {risk.label}
          </span>
        )}
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
          <button className="sm" onClick={onToggle}>
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
                ref={audioPlayerRef}
                path={sub.file_path}
                onReady={() => setAudioState("ready")}
                onError={() => setAudioState("error")}
                onRangeEnd={() => setActiveSpanKey("")}
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
              ref={noteRef}
              rows={3}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              placeholder="记录听辨依据、错漏位置或改判原因（可选）"
              disabled={Boolean(sub.human_result) && reviewMode === "view"}
            />
          </label>

          {sub.structured_score && (
            <section className="point-review">
              <div className="point-review-head">
                <div>
                  <h3>评分点证据</h3>
                  <p>
                    共 {sub.structured_score.points.length} 点 · 机器置信度{" "}
                    {Math.round(sub.structured_score.confidence * 100)}%
                    {sub.structured_score.review_revision
                      ? ` · 已保存老师复核 v${sub.structured_score.review_revision}`
                      : " · 尚未由老师逐点确认"}
                  </p>
                </div>
                {(sub.human_result ? reviewMode !== "view" : true) && (
                  <label className="point-review-toggle">
                    <input
                      type="checkbox"
                      checked={pointReviewEnabled}
                      onChange={(event) => togglePointReview(event.target.checked)}
                    />
                    同时确认逐评分点
                  </label>
                )}
              </div>
              {!pointReviewEnabled && (!sub.human_result || reviewMode !== "view") && (
                <div className="evidence-hint">
                  未开启时，本次只确认总体通过/不通过，机器逐点分析保持“未核实”。
                </div>
              )}
              <div className="point-list">
                {sub.structured_score.points.map((point) => {
                  const draft = pointDrafts[point.point_result_id] ?? pointDraft(point);
                  const evidenceSpans = validEvidenceSpans(point);
                  const editable = pointReviewEnabled && (!sub.human_result || reviewMode !== "view");
                  const shownState =
                    sub.human_result && reviewMode === "view" && point.teacher_state
                      ? point.teacher_state
                      : draft.state;
                  return (
                    <article className="point-item" key={point.point_result_id}>
                      <div className="point-item-main">
                        <span className={`point-state ${shownState}`}>
                          {POINT_STATE_LABEL[shownState]}
                        </span>
                        <div>
                          <b>{point.canonical_text}</b>
                          <p>{point.reason}</p>
                          {evidenceSpans.length > 0 ? (
                            <div className="point-spans">
                              {evidenceSpans.map((span) => {
                                const key = `${point.point_result_id}:${span.index}`;
                                return (
                                  <button
                                    type="button"
                                    className={activeSpanKey === key ? "active" : ""}
                                    key={key}
                                    disabled={audioState !== "ready"}
                                    onClick={() => playEvidenceSpan(point, span)}
                                    title={
                                      audioState === "ready"
                                        ? "跳到并只播放这段录音"
                                        : "录音准备好后可跳播"
                                    }
                                  >
                                    <span aria-hidden="true">▶</span>
                                    {formatTimestamp(span.start_ms)}–
                                    {formatTimestamp(span.end_ms)}
                                    {span.text ? <em>「{span.text}」</em> : null}
                                  </button>
                                );
                              })}
                            </div>
                          ) : (
                            <small>
                              {point.evidence_spans.length
                                ? "时间片段不可用，请对照 ASR 原文或完整录音"
                                : "未定位到对应片段，请对照 ASR 原文或完整录音"}
                            </small>
                          )}
                        </div>
                        <span className="point-confidence">
                          {Math.round(point.confidence * 100)}%
                        </span>
                      </div>
                      {editable && (
                        <div className="point-edit">
                          <select
                            value={
                              draft.confirmation === "accepted"
                                ? "accepted"
                                : `corrected:${draft.state}`
                            }
                            onChange={(event) => {
                              const value = event.target.value;
                              setPointDrafts((current) => ({
                                ...current,
                                [point.point_result_id]:
                                  value === "accepted"
                                    ? {
                                        confirmation: "accepted",
                                        state: point.machine_state,
                                        note: "",
                                      }
                                    : {
                                        ...draft,
                                        confirmation: "corrected",
                                        state: value.replace(
                                          "corrected:",
                                          "",
                                        ) as RecitationPointState,
                                      },
                              }));
                            }}
                          >
                            <option value="accepted">
                              接受机器判断：{POINT_STATE_LABEL[point.machine_state]}
                            </option>
                            {Object.entries(POINT_STATE_LABEL).map(([state, label]) => (
                              <option key={state} value={`corrected:${state}`}>
                                人工修正为：{label}
                              </option>
                            ))}
                          </select>
                          {draft.confirmation === "corrected" && (
                            <input
                              value={draft.note}
                              onChange={(event) =>
                                setPointDrafts((current) => ({
                                  ...current,
                                  [point.point_result_id]: {
                                    ...draft,
                                    note: event.target.value,
                                  },
                                }))
                              }
                              placeholder="修正原因（必填）"
                            />
                          )}
                        </div>
                      )}
                      {!editable && point.teacher_confirmation_level && (
                        <div className="point-reviewed-note">
                          老师{point.teacher_confirmation_level === "accepted" ? "已接受" : "已修正"}
                          {point.teacher_note ? ` · ${point.teacher_note}` : ""}
                        </div>
                      )}
                    </article>
                  );
                })}
              </div>
              {pointError && <div className="evidence-error">{pointError}</div>}
            </section>
          )}

          {!sub.human_result && (
            <div className="review-shortcuts" aria-label="终审快捷键说明">
              快捷键：J / K 切换疑点 · 空格播放 / 暂停 · P 通过 · F 不通过 · N 备注
            </div>
          )}

          {sub.human_result ? (
            reviewMode === "rejudge" ? (
              <div className="evidence-actions">
                <div className="evidence-hint">
                  改判会先回滚旧副作用，再应用新结论；状态已漂移时系统会拒绝自动覆盖。
                </div>
                <div className="spacer" />
                <button disabled={busy} onClick={() => setReviewMode("view")}>取消</button>
                <button
                  className="primary"
                  disabled={busy || !canDecide}
                  onClick={() => decide(sub.human_result === "pass" ? "fail" : "pass")}
                >
                  确认改判为{sub.human_result === "pass" ? "不通过" : "通过"}
                </button>
              </div>
            ) : reviewMode === "points" ? (
              <div className="evidence-actions">
                <div className="evidence-hint">
                  保存后只补充或更新逐评分点复核，不会重复推进复习卡。
                </div>
                <div className="spacer" />
                <button disabled={busy} onClick={() => setReviewMode("view")}>取消</button>
                <button
                  className="primary"
                  disabled={busy || !canDecide || !pointReviewEnabled}
                  onClick={() => decide(sub.human_result as "pass" | "fail")}
                >
                  保存逐点评审
                </button>
              </div>
            ) : (
              <div className="reviewed-result">
                <span>
                  已终审：{sub.human_result === "pass" ? "通过" : sub.human_result === "fail" ? "不通过" : "重开"}
                  {sub.human_note ? ` · ${sub.human_note}` : ""}
                </span>
                <div className="reviewed-actions">
                  {sub.human_result !== "reopen" && sub.structured_score && (
                    <button disabled={busy} onClick={beginPointEdit}>
                      {sub.structured_score.review_revision ? "更新逐点复核" : "补充逐点复核"}
                    </button>
                  )}
                  {sub.human_result !== "reopen" && (
                    <button disabled={busy} onClick={beginRejudge}>发起改判</button>
                  )}
                </div>
              </div>
            )
          ) : (
            <div className="evidence-actions">
              <div className="evidence-hint">
                {canDecide ? "证据已就绪，可以终审。" : "需录音可读、ASR/答案非空且答案版本一致后才能确认。"}
              </div>
              <div className="spacer" />
              <button disabled={busy} onClick={() => decide("reopen")}>重开待重交</button>
              <button disabled={busy || !canDecide} onClick={() => decide(sub.pass ? "fail" : "pass")}>
                判为{sub.pass ? "不通过" : "通过"}
              </button>
              <button className="primary" disabled={busy || !canDecide} onClick={() => decide(sub.pass ? "pass" : "fail")}>
                确认系统建议
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
