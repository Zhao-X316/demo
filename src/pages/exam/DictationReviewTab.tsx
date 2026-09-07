import { convertFileSrc } from "@tauri-apps/api/core";
import {
  DictationWorkbench,
  DictationWorkbenchRow,
} from "../../api/exam";
import { useDictationReviewController } from "./useDictationReviewController";

function dictationStateLabel(row: DictationWorkbenchRow) {
  if (row.point_result === "exact") return "与标准答案一致";
  if (row.point_result === "accepted_variant") return "可接受写法";
  if (row.result_state === "not_written") return "疑似未写";
  if (row.result_state === "unreadable") return "字迹无法辨认";
  if (row.result_state === "recognize_failed") return "识别失败";
  if (row.result_state === "ambiguous_final") return "涂改后答案不明确";
  return "与标准答案有分歧";
}

function DictationReviewTab({
  workbench,
  onDone,
  onError,
}: {
  workbench: DictationWorkbench;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const {
    view: {
      assessmentVersions,
      assessmentVersionId,
      assessmentItemId,
      itemOptions,
      rows,
      attempts,
      confirmedCount,
      eligibleRows,
      exceptionCount,
      busy,
      corrections,
      manualScores,
      manualNotes,
      manualEvidence,
    },
    actions: {
      selectAssessment,
      selectItem,
      changeCorrection,
      changeManualScore,
      changeManualNote,
      changeManualEvidence,
      correct,
      retry,
      acceptOne,
      gradeOne,
      acceptStrictBatch,
      publishAttempt,
    },
  } = useDictationReviewController(workbench, onDone, onError);

  if (!workbench.rows.length) {
    return (
      <div className="empty-state objective-empty">
        <b>还没有可终审的默写结果。</b>
        <span>上传固定默写后，系统会按已确认模板裁出每个答案区；精确结果可批量确认，分歧只需老师查看原图。</span>
      </div>
    );
  }

  return (
    <>
      <section className="objective-toolbar exam-card">
        <label className="field">
          <span className="fl">作业版本</span>
          <select value={assessmentVersionId} onChange={(event) => selectAssessment(Number(event.target.value))}>
            {assessmentVersions.map((version) => <option value={version.id} key={version.id}>{version.title} · v{version.id}</option>)}
          </select>
        </label>
        <label className="field">
          <span className="fl">按题终审</span>
          <select value={assessmentItemId} onChange={(event) => selectItem(Number(event.target.value))}>
            {itemOptions.map((row) => (
              <option value={row.assessment_item_id} key={row.assessment_item_id}>
                第{row.question_no}题 · {row.question_stem.slice(0, 34)}
              </option>
            ))}
          </select>
        </label>
        <div className="objective-stats">
          <span><b>{rows.length}</b> 份作答</span>
          <span className="ok-text"><b>{confirmedCount}</b> 已确认</span>
          <span><b>{eligibleRows.length}</b> 可严格批量</span>
          <span className={exceptionCount ? "bad-text" : ""}><b>{exceptionCount}</b> 需单独处理</span>
        </div>
        <button className="primary" disabled={busy || !eligibleRows.length} onClick={() => void acceptStrictBatch()}>
          {busy ? "处理中…" : `确认 ${eligibleRows.length} 条精确结果`}
        </button>
        <div className="meta objective-batch-note">只纳入置信度不低于 0.95 的精确答案；未写、模糊、涂改、分歧和已终审项都会逐条排除。</div>
      </section>

      <div className="sech">本题证据 <span className="n">按学号排序</span></div>
      <div className="objective-review-list">
        {rows.map((row) => {
          const editable = row.result_state === "recognized";
          const value = corrections[row.answer_region_revision_id]
            ?? row.teacher_corrected_text
            ?? row.raw_ocr_text
            ?? "";
          return (
            <article className={row.current_transcription_confirmed ? "objective-review-row confirmed" : "objective-review-row"} key={row.transcription_revision_id}>
              <div className="objective-student">
                <b>{row.student_name}</b>
                <span>{row.student_no}号 · 第{row.question_no}题</span>
                <span>满分 {row.max_score} 分</span>
                <span className={row.current_transcription_confirmed ? "tag pass" : row.requires_teacher_review ? "tag fail" : "tag wait"}>
                  {row.current_transcription_confirmed ? "已终审" : row.requires_teacher_review ? "需单独处理" : "可确认"}
                </span>
              </div>
              <div className="objective-evidence">
                {row.crop_path
                  ? <img className="objective-crop" src={convertFileSrc(row.crop_path)} alt={`${row.student_name} 第${row.question_no}题默写`} />
                  : <div className="objective-crop missing">裁剪图不可用</div>}
                <div className="objective-facts">
                  <span>机器判断 <b>{dictationStateLabel(row)}</b></span>
                  <span>机器原文 <b>{row.raw_ocr_text || "—"}</b></span>
                  {row.teacher_corrected_text && <span>老师校正 <b>{row.teacher_corrected_text}</b></span>}
                  <span>标准内容 <b>{row.canonical_text}</b></span>
                  <span>置信度 <b>{row.confidence == null ? "—" : `${Math.round(row.confidence * 100)}%`}</b></span>
                  <span>建议得分 <b>{row.suggested_score == null ? "—" : `${row.suggested_score} / ${row.max_score}`}</b></span>
                  {row.accepted_variants.length > 0 && <span>可接受写法 <b>{row.accepted_variants.join("、")}</b></span>}
                  {row.current_transcription_confirmed && <span>老师终审 <b>{row.teacher_score} 分 · {row.review_mode === "strict_batch" ? "严格批量" : "逐条确认"}</b></span>}
                </div>
              </div>
              <div className="objective-actions">
                {!row.current_transcription_confirmed && row.suggested_score != null && (
                  <button className="primary" disabled={busy} onClick={() => void acceptOne(row)}>接受本条建议</button>
                )}
                {!row.current_transcription_confirmed && editable && row.requires_teacher_review && (
                  <details>
                    <summary>校正机器原文</summary>
                    <input
                      value={value}
                      aria-label={`${row.student_name} 第${row.question_no}题实际书写`}
                      onChange={(event) => changeCorrection(row.answer_region_revision_id, event.target.value)}
                    />
                    <button
                      disabled={busy}
                      onClick={() => void correct(row)}
                    >
                      按原图保存实际书写
                    </button>
                  </details>
                )}
                {!row.current_transcription_confirmed && row.result_state === "recognize_failed" && (
                  <button disabled={busy} onClick={() => void retry(row)}>重新识别本题</button>
                )}
                {!row.current_transcription_confirmed && (
                  <details>
                    <summary>人工记分 / 补录</summary>
                    <label className="field">
                      <span className="fl">得分（满分 {row.max_score}）</span>
                      <input type="number" min="0" max={row.max_score} step="0.5"
                        value={manualScores[row.transcription_revision_id] ?? String(row.suggested_score ?? "")}
                        onChange={(event) => changeManualScore(row.transcription_revision_id, event.target.value)} />
                    </label>
                    <label className="field">
                      <span className="fl">学生实际书写（可选）</span>
                      <input type="text" placeholder="按原图补录，不覆盖 OCR"
                        value={manualEvidence[row.transcription_revision_id] ?? row.teacher_corrected_text ?? ""}
                        onChange={(event) => changeManualEvidence(row.transcription_revision_id, event.target.value)} />
                    </label>
                    <label className="field">
                      <span className="fl">判定依据（必填）</span>
                      <input type="text" placeholder="如：原图未写，记 0 分"
                        value={manualNotes[row.transcription_revision_id] ?? ""}
                        onChange={(event) => changeManualNote(row.transcription_revision_id, event.target.value)} />
                    </label>
                    <button disabled={busy} onClick={() => void gradeOne(row)}>保存人工 revision</button>
                  </details>
                )}
              </div>
            </article>
          );
        })}
      </div>

      <div className="sech">整份默写发布 <span className="n">终审完成不等于已发布</span></div>
      <div className="objective-publish-grid">
        {attempts.map((attempt) => (
          <article className="exam-card objective-attempt" key={attempt.attempt_id}>
            <div className="exam-card-head">
              <div><b>{attempt.student_name}</b><div className="meta">{attempt.student_no}</div></div>
              <span className={attempt.attempt_state === "published" ? "tag pass" : attempt.can_publish ? "tag wait" : "tag"}>
                {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "待发布" : "终审中"}
              </span>
            </div>
            <div className="objective-attempt-score"><b>{attempt.teacher_total_score}</b><span>/ {attempt.max_total_score} 分</span></div>
            <div className="meta">已终审 {attempt.confirmed_count} / {attempt.item_count} 题</div>
            {attempt.published_total_score != null && <div className="meta">当前已发布总分：{attempt.published_total_score}</div>}
            <button className="primary" disabled={busy || !attempt.can_publish} onClick={() => void publishAttempt(attempt.attempt_id, attempt.student_name)}>
              {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "确认发布整份默写" : "完成全部终审后发布"}
            </button>
          </article>
        ))}
      </div>
    </>
  );
}

export { DictationReviewTab };
