import { useEffect, useMemo, useState } from "react";
import { TaskCard, TeacherPointReviewInput, humanDecide } from "../api/dashboard";
import {
  ImportHistoryRow,
  importHistory,
  recitationSubmissionDetail,
} from "../api/records";
import { TaskRow } from "./Today";

export default function Records() {
  const [rows, setRows] = useState<ImportHistoryRow[] | null>(null);
  const [q, setQ] = useState("");
  const [err, setErr] = useState("");
  const [detail, setDetail] = useState<TaskCard | null>(null);
  const [detailLoadingId, setDetailLoadingId] = useState<number | null>(null);
  const [reviewBusy, setReviewBusy] = useState(false);

  const loadRows = async () => {
    try {
      setRows(await importHistory());
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    loadRows();
  }, []);

  const openDetail = async (submissionId: number) => {
    if (detail?.submission?.submission_id === submissionId) {
      setDetail(null);
      return;
    }
    setErr("");
    setDetailLoadingId(submissionId);
    try {
      setDetail(await recitationSubmissionDetail(submissionId));
    } catch (e) {
      setErr(String(e));
    }
    setDetailLoadingId(null);
  };

  const decide = async (
    task: TaskCard,
    result: "pass" | "fail" | "reopen",
    note?: string,
    pointReview?: TeacherPointReviewInput,
  ) => {
    const submissionId = task.submission?.submission_id;
    if (!submissionId) return;
    setErr("");
    setReviewBusy(true);
    try {
      await humanDecide(submissionId, result, note, pointReview);
      setDetail(await recitationSubmissionDetail(submissionId));
      await loadRows();
    } catch (e) {
      setErr(String(e));
    }
    setReviewBusy(false);
  };

  const filtered = useMemo(
    () =>
      (rows ?? []).filter(
        (r) =>
          !q ||
          r.file_name.includes(q) ||
          (r.student ?? "").includes(q) ||
          (r.content ?? "").includes(q),
      ),
    [rows, q],
  );

  return (
    <div className="page">
      <div className="page-head">
        <h1>记录</h1>
        <span className="date">导入历史 · 共 {rows?.length ?? 0} 个文件</span>
      </div>
      <div className="sub">
        所有导入过的录音都留底（切走再回来不丢）。原文件名经智能识别后改成 <code>日期_学号_姓名_编号</code>。
      </div>
      {err && <div className="error">{err}</div>}
      <div className="toolbar">
        <input className="search" placeholder="搜索 文件名 / 学生 / 内容…" value={q} onChange={(e) => setQ(e.target.value)} />
        <div className="spacer" />
        <span className="muted" style={{ fontSize: 13 }}>显示 {filtered.length} 条</span>
      </div>

      {rows == null && <div className="loading">加载导入历史…</div>}
      {rows != null && (
        <table className="tbl">
          <thead>
            <tr>
              <th>文件名（识别后）</th>
              <th>学生</th>
              <th>内容</th>
              <th>状态</th>
              <th>证据</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((r) => (
              <tr key={r.submission_id}>
                <td className="filecell">{r.file_name}</td>
                <td>{r.student ?? "—"}</td>
                <td>{r.content ?? "—"}</td>
                <td style={{ color: r.status.includes("未识别") ? "var(--bad)" : "var(--text-2)" }}>
                  {r.status}
                </td>
                <td>
                  {r.has_evidence_detail ? (
                    <button
                      className="sm"
                      disabled={detailLoadingId !== null}
                      onClick={() => openDetail(r.submission_id)}
                    >
                      {detailLoadingId === r.submission_id
                        ? "加载中…"
                        : detail?.submission?.submission_id === r.submission_id
                          ? "收起"
                          : "查看终审记录"}
                    </button>
                  ) : (
                    <span className="muted">未绑定任务</span>
                  )}
                </td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={5} className="muted" style={{ padding: "24px 8px", textAlign: "center" }}>
                  暂无记录
                </td>
              </tr>
            )}
          </tbody>
        </table>
      )}
      {detail && (
        <div style={{ marginTop: 18 }}>
          <div className="sech">历史终审证据</div>
          <TaskRow
            t={detail}
            busy={reviewBusy}
            expanded
            onToggle={() => setDetail(null)}
            onDecide={decide}
          />
        </div>
      )}
    </div>
  );
}
