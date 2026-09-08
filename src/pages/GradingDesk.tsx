import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AutonameResult,
  StageResult,
  asrAndScore,
  importAutoname,
  importStage,
} from "../api/importing";
import {
  Anomaly,
  RecognitionFailure,
  Suggest,
  anomaliesList,
  anomalyReassign,
  recognitionFailuresList,
  recognitionRelocate,
  recognitionVoid,
  suggestMatch,
} from "../api/anomaly";
import { RecContent, Student, contentsList, studentsList } from "../api/manage";
import { AudioPlayer } from "../components/AudioPlayer";
import { Avatar } from "../components/ui";

const AUDIO_EXTENSIONS = ["m4a", "mp3", "wav", "aac", "amr", "ogg"] as const;

function hasSupportedAudioExtension(path: string): boolean {
  const clean = path.split(/[?#]/, 1)[0].toLowerCase();
  return AUDIO_EXTENSIONS.some((ext) => clean.endsWith(`.${ext}`));
}

function asrText(meta: string | null): string {
  if (!meta) return "";
  try {
    const o = JSON.parse(meta) as { asr?: unknown };
    return typeof o.asr === "string" ? o.asr : "";
  } catch {
    return "";
  }
}

type Sel =
  | { kind: "result"; i: number }
  | { kind: "anomaly"; id: number }
  | { kind: "failure"; id: number }
  | null;

export default function GradingDesk({ initialSubmissionId,onDirtyChange }: { initialSubmissionId?: number;onDirtyChange?:(dirty:boolean)=>void }) {
  const [staged, setStaged] = useState<StageResult[]>([]);
  const [force, setForce] = useState(false);
  const [results, setResults] = useState<AutonameResult[]>([]);
  const [anomalies, setAnomalies] = useState<Anomaly[]>([]);
  const [failures, setFailures] = useState<RecognitionFailure[]>([]);
  const [suggest, setSuggest] = useState<Record<number, Suggest>>({});
  const [students, setStudents] = useState<Student[]>([]);
  const [contents, setContents] = useState<RecContent[]>([]);
  const [sel, setSel] = useState<Sel>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const loadPending = async () => {
    try {
      const [list, failed] = await Promise.all([anomaliesList(), recognitionFailuresList()]);
      setAnomalies(list);
      setFailures(failed);
      if (initialSubmissionId) {
        if (failed.some(row => row.submission_id === initialSubmissionId)) setSel({kind:"failure",id:initialSubmissionId});
        else if (list.some(row => row.submission_id === initialSubmissionId)) setSel({kind:"anomaly",id:initialSubmissionId});
      }
      const sug: Record<number, Suggest> = {};
      for (const r of list) {
        const t = asrText(r.parsed_meta);
        if (t) {
          try {
            sug[r.submission_id] = await suggestMatch(t);
          } catch {
            /* ignore */
          }
        }
      }
      setSuggest(sug);
    } catch (e) {
      setErr(String(e));
    }
  };
  useEffect(() => {
    loadPending();
    studentsList().then(setStudents).catch(() => undefined);
    contentsList().then(setContents).catch(() => undefined);
  }, []);

  const pickAndStage = async () => {
    setErr("");
    try {
      // macOS 原生扩展过滤在部分系统版本会把合法音频全部置灰；
      // 保持文件选择可用，并在进入导入管线前执行同一白名单校验。
      const selFiles = await open({ multiple: true });
      if (!selFiles) return;
      const paths = Array.isArray(selFiles) ? selFiles : [selFiles];
      const unsupported = paths.filter((path) => !hasSupportedAudioExtension(path));
      if (unsupported.length) {
        setErr(`仅支持 ${AUDIO_EXTENSIONS.join(" / ")} 音频；本次未导入 ${unsupported.length} 个不支持的文件。`);
        return;
      }
      const res = await importStage(paths, force);
      setStaged((s) => [...s, ...res]);
      const dup = res.filter((r) => r.status === "duplicate").length;
      const bad = res.filter((r) => r.status === "error").length;
      if (res.every((r) => r.status !== "staged")) {
        setErr(
          `没有新文件进入队列：${dup ? `${dup} 个是重复（之前导过同一录音，勾"忽略重复"可强制导入）` : ""}${bad ? ` ${bad} 个读取失败` : ""}`,
        );
      }
    } catch (e) {
      setErr("打开文件 / 导入失败：" + String(e));
    }
  };
  useEffect(()=>{onDirtyChange?.(busy || staged.some(row=>row.status==="staged"||row.status==="analyzing"));},[busy,staged]);
  const queued = staged.filter((s) => s.status === "staged");
  const analyze = async () => {
    setBusy(true);
    setErr("");
    let failed = 0;
    try {
      for (const item of queued) {
        setStaged((rows) =>
          rows.map((r) =>
            r.file === item.file ? { ...r, status: "analyzing", detail: "正在识别与评分…" } : r,
          ),
        );
        try {
          const res = await importAutoname([item.file], force);
          const one = res[0];
          if (one) {
            setResults((r) => [one, ...r]);
            setStaged((rows) =>
              rows.map((r) =>
                r.file === item.file ? { ...r, status: "done", detail: one.detail } : r,
              ),
            );
          }
        } catch (e) {
          failed += 1;
          setStaged((rows) =>
            rows.map((r) =>
              r.file === item.file ? { ...r, status: "error", detail: String(e) } : r,
            ),
          );
        }
      }
      setStaged((rows) => rows.filter((r) => r.status !== "done"));
      await loadPending(); // unmatched / failed 都会持久化进待处理区
      if (failed) setErr(`有 ${failed} 个文件分析失败，其余已继续处理`);
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };

  const cur = sel;
  const curAnomaly = cur?.kind === "anomaly" ? anomalies.find((a) => a.submission_id === cur.id) : null;
  const curFailure = cur?.kind === "failure" ? failures.find((f) => f.submission_id === cur.id) : null;
  const curResult = cur?.kind === "result" ? results[cur.i] : null;

  return (
    <div className="page">
      <div className="page-head">
        <h1>批改台</h1>
        <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
          <label className="field check" style={{ fontSize: 12, color: "var(--muted)", marginRight: 4 }} title="同一录音文件默认只导一次；勾上可强制重复导入（测试或重交时用）">
            <input type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} />
            忽略重复
          </label>
          <button onClick={pickAndStage}>＋ 导入录音</button>
          <button className="primary" disabled={busy || !queued.length} onClick={analyze}>
            {busy ? "分析中…" : `开始分析${queued.length ? ` (${queued.length})` : ""}`}
          </button>
        </div>
      </div>
      <div className="sub">
        录音 → 智能识别学生与内容 → 逐条确认。识别不了的落进下方「异常池」，由你下拉指认。
      </div>
      {err && <div className="error">{err}</div>}

      <div className="desk">
        <div className="queue">
          {staged.length > 0 && (
            <>
              <div className="sech">待分析队列<span className="n">{queued.length}</span></div>
              {staged.map((s, i) => (
                <div className="qitem" key={"st" + i}>
                  <div className="qfile">{s.file.split("/").pop()}</div>
                  <div className="qstat" style={s.status !== "staged" ? { color: "var(--warn)" } : {}}>
                    {s.status === "staged"
                      ? "已排队，点「开始分析」"
                      : s.status === "analyzing"
                        ? "正在分析…"
                      : s.status === "duplicate"
                        ? "重复（之前导过同一录音，已跳过）"
                        : s.status === "done"
                          ? `已完成 · ${s.detail}`
                          : `失败 · ${s.detail}`}
                  </div>
                </div>
              ))}
            </>
          )}
          {results.length > 0 && (
            <>
              <div className="sech">本次识别<span className="n">{results.length}</span></div>
              {results.map((r, i) => (
                <div
                  className={"qitem" + (cur?.kind === "result" && cur.i === i ? " on" : "")}
                  key={"r" + i}
                  onClick={() => setSel({ kind: "result", i })}
                >
                  <div className="qfile">{r.new_name ?? r.file.split("/").pop()}</div>
                  <div className="qstat">
                    {r.status === "scored" || r.status === "rescored" ? (
                      <span style={{ color: r.pass ? "var(--ok)" : "var(--bad)" }}>
                        ● {r.status === "rescored" ? "重析" : ""}{r.student} · {r.pass ? "通过" : "不通过"} {r.accuracy ?? ""}
                      </span>
                    ) : r.status === "unmatched" ? (
                      <span style={{ color: "var(--bad)" }}>● 未识别 · 见异常池</span>
                    ) : (
                      <span>{r.detail}</span>
                    )}
                  </div>
                </div>
              ))}
            </>
          )}
          <div className="sech">
            识别失败 · 待处理<span className="n">{failures.length}</span>
          </div>
          {failures.length === 0 && <div className="qstat" style={{ padding: "4px 2px" }}>无</div>}
          {failures.map((failure) => (
            <div
              className={"qitem" + (cur?.kind === "failure" && cur.id === failure.submission_id ? " on" : "")}
              key={"f" + failure.submission_id}
              onClick={() => setSel({ kind: "failure", id: failure.submission_id })}
            >
              <div className="qfile">{failure.file_path.split("/").pop()}</div>
              <div className="qstat" style={{ color: "var(--bad)" }}>
                ● {failure.file_missing ? "原文件丢失" : failure.retryable ? "可重试" : "需作废/换录音"} · 已试 {failure.attempts} 次
              </div>
            </div>
          ))}
          <div className="sech">
            异常池 · 待改派<span className="n">{anomalies.length}</span>
          </div>
          {anomalies.length === 0 && <div className="qstat" style={{ padding: "4px 2px" }}>无</div>}
          {anomalies.map((a) => (
            <div
              className={"qitem" + (cur?.kind === "anomaly" && cur.id === a.submission_id ? " on" : "")}
              key={a.submission_id}
              onClick={() => setSel({ kind: "anomaly", id: a.submission_id })}
            >
              <div className="qfile">{a.file_path.split("/").pop()}</div>
              <div className="qstat" style={{ color: "var(--bad)" }}>● 未识别 · 待改派</div>
            </div>
          ))}
        </div>

        <div className="detail">
          {!cur && <div className="muted">选择左侧一条录音查看</div>}
          {curResult && <ResultDetail r={curResult} />}
          {curFailure && (
            <FailureDetail
              failure={curFailure}
              onResolved={() => {
                setSel(null);
                loadPending();
              }}
              onRefresh={loadPending}
            />
          )}
          {curAnomaly && (
            <AnomalyDetail
              a={curAnomaly}
              sug={suggest[curAnomaly.submission_id]}
              students={students}
              contents={contents}
              onDone={() => {
                setSel(null);
                loadPending();
              }}
              onRefresh={loadPending}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function ResultDetail({ r }: { r: AutonameResult }) {
  if (r.status !== "scored" && r.status !== "rescored") {
    return (
      <div>
        <div className="who" style={{ fontSize: 15, marginBottom: 8 }}>
          {r.status === "unmatched" ? "未识别出学生/内容" : r.detail}
        </div>
        <div className="meta">{r.file.split("/").pop()}</div>
        {r.status === "unmatched" && (
          <div className="hint" style={{ marginTop: 14 }}>
            这条已进下方「异常池」，请在异常池里下拉指认后改派。
          </div>
        )}
      </div>
    );
  }
  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 14 }}>
        <Avatar name={r.student ?? "?"} />
        <div>
          <div className="who" style={{ fontSize: 15 }}>{r.student}</div>
          <div className="meta">{r.new_name ?? r.file.split("/").pop()}</div>
        </div>
        <div className="spacer" />
        <span className={"acc num " + (r.pass ? "p" : "f")} style={{ fontSize: 28 }}>
          {r.accuracy ?? ""}
        </span>
        {r.pass ? <span className="tag pass">通过</span> : <span className="tag fail">不通过</span>}
      </div>
      <div className="kv">
        <b>识别内容</b>
        <span>{r.content}</span>
      </div>
      <div className="kv">
        <b>结果</b>
        <span>
          正确率 {r.accuracy ?? "—"}% · {r.pass ? "通过，已排复习" : "未通过，已排明日补背"}
        </span>
      </div>
      <div className="hint" style={{ marginTop: 14 }}>
        系统判定 = {r.pass ? "通过" : "不通过"}。如需人工改判，在「今日」逐条终审（保留老师最终判定权）。
      </div>
    </div>
  );
}

function AnomalyDetail({
  a,
  sug,
  students,
  contents,
  onDone,
  onRefresh,
}: {
  a: Anomaly;
  sug?: Suggest;
  students: Student[];
  contents: RecContent[];
  onDone: () => void;
  onRefresh: () => Promise<void>;
}) {
  const [sno, setSno] = useState("");
  const [cno, setCno] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const asr = asrText(a.parsed_meta);

  const reassign = async () => {
    setBusy(true);
    setErr("");
    try {
      await anomalyReassign(a.submission_id, sno || undefined, cno || undefined);
      await asrAndScore(a.submission_id);
      onDone();
    } catch (e) {
      setErr(String(e));
      await onRefresh();
      setBusy(false);
    }
  };

  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 14 }}>
        <Avatar name="?" q />
        <div>
          <div className="who" style={{ fontSize: 15 }}>识别不出学生 / 内容</div>
          <div className="meta">{a.file_path.split("/").pop()}</div>
        </div>
      </div>
      {err && <div className="error">{err}</div>}
      {asr && (
        <div className="kv" style={{ alignItems: "flex-start" }}>
          <b>识别原文</b>
          <div style={{ flex: 1 }}>
            <div className="asrbox">{asr}</div>
          </div>
        </div>
      )}
      <AudioPlayer path={a.file_path} />
      <div className="sub" style={{ margin: "14px 0 8px" }}>系统拿不准，下拉指认（不用手敲学号）：</div>
      <div style={{ display: "flex", gap: 10, marginBottom: 12 }}>
        <select value={sno} onChange={(e) => setSno(e.target.value)}>
          <option value="">选择学生…</option>
          {students.filter((s) => s.enabled).map((s) => (
            <option key={s.id} value={s.student_no}>
              {s.name}（{s.student_no}）
            </option>
          ))}
        </select>
        <select value={cno} onChange={(e) => setCno(e.target.value)}>
          <option value="">选择内容…</option>
          {contents.filter((c) => c.enabled).map((c) => (
            <option key={c.id} value={c.content_no}>
              {c.content_no} {c.title}
            </option>
          ))}
        </select>
      </div>
      {sug && (sug.student_name || sug.contents.length > 0) && (
        <div className="hint" style={{ marginBottom: 12 }}>
          建议：
          {sug.student_name && <> 学生像「{sug.student_name}」 ·</>}
          {sug.contents[0] && (
            <>
              {" "}
              内容像「{sug.contents[0].content_no} {sug.contents[0].title}」（匹配度{" "}
              {Math.round(sug.contents[0].score)}%）·{" "}
              <button
                className="link"
                onClick={() => {
                  if (sug.student_no) setSno(sug.student_no);
                  setCno(sug.contents[0].content_no);
                }}
              >
                采纳
              </button>
            </>
          )}
        </div>
      )}
      <button className="primary" disabled={busy} onClick={reassign}>
        {busy ? "处理中…" : "改派并识别"}
      </button>
    </div>
  );
}

function FailureDetail({
  failure,
  onResolved,
  onRefresh,
}: {
  failure: RecognitionFailure;
  onResolved: () => void;
  onRefresh: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const retry = async () => {
    setBusy(true);
    setErr("");
    try {
      await asrAndScore(failure.submission_id);
      onResolved();
    } catch (e) {
      setErr(String(e));
      await onRefresh();
      setBusy(false);
    }
  };

  const relocate = async () => {
    setErr("");
    const selected = await open({ multiple: false });
    if (!selected || Array.isArray(selected)) return;
    if (!hasSupportedAudioExtension(selected)) {
      setErr(`仅支持 ${AUDIO_EXTENSIONS.join(" / ")} 音频，请重新选择。`);
      return;
    }
    setBusy(true);
    try {
      await recognitionRelocate(failure.submission_id, selected);
      await asrAndScore(failure.submission_id);
      onResolved();
    } catch (e) {
      setErr(String(e));
      await onRefresh();
      setBusy(false);
    }
  };

  const voidRecord = async () => {
    if (!window.confirm("作废这条未终审提交？旧记录会保留；若它是任务唯一提交，任务将重开等待新录音。")) return;
    setBusy(true);
    setErr("");
    try {
      const reopened = await recognitionVoid(failure.submission_id);
      if (reopened) {
        setErr("旧提交已作废，原任务已重开；请用左上角“导入录音”提交新文件。");
      }
      onResolved();
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 14 }}>
        <Avatar name="!" q />
        <div>
          <div className="who" style={{ fontSize: 15 }}>ASR 识别失败</div>
          <div className="meta">{failure.file_path.split("/").pop()}</div>
        </div>
        <div className="spacer" />
        <span className="tag fail">第 {failure.attempts} 次</span>
      </div>
      {err && <div className="error">{err}</div>}
      <div className="kv"><b>失败类型</b><span>{failure.error_code}</span></div>
      <div className="kv"><b>失败时间</b><span>{failure.failed_at}</span></div>
      <div className="asrbox" style={{ color: "var(--bad)" }}>{failure.error_message}</div>
      <AudioPlayer path={failure.file_path} />
      <div className="hint" style={{ marginTop: 14 }}>
        {failure.file_missing
          ? "原路径已不可用：请选择同一份录音重新定位；系统会校验 hash，不同录音必须新建提交。"
          : failure.retryable
            ? "修复网络或凭据后可重试；重试复用同一 submission 和 file hash，不会新增记录。"
            : "该错误不建议直接重试。可作废旧记录，再从左上角导入一份新录音。"}
      </div>
      <div className="divln" style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
        <button className="danger" disabled={busy} onClick={voidRecord}>作废旧记录</button>
        {failure.file_missing && <button disabled={busy} onClick={relocate}>重新定位并重试</button>}
        {!failure.file_missing && failure.retryable && (
          <button className="primary" disabled={busy} onClick={retry}>重试识别</button>
        )}
      </div>
    </div>
  );
}
