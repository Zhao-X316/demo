import { convertFileSrc } from "@tauri-apps/api/core";
import {
  ObjectiveWorkbench,
  ObjectiveWorkbenchRow,
} from "../../api/exam";
import { useObjectiveReviewController } from "./useObjectiveReviewController";

const OBJECTIVE_TYPE_LABEL: Record<string, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
};

const OBSERVATION_LABEL: Record<string, string> = {
  recognized: "识别清晰",
  blank: "疑似空白",
  altered: "存在涂改",
  low_confidence: "低置信度",
  failed: "识别失败",
};

const EXCLUSION_LABEL: Record<string, string> = {
  ALREADY_CONFIRMED: "已终审",
  OBSERVATION_NOT_RECOGNIZED: "识别状态不合格",
  LOW_CONFIDENCE: "置信度不足",
  NOT_BATCH_ELIGIBLE: "不满足批量规则",
  UNSCORED: "机器无法确定计分",
};

function displayObservedAnswer(row: ObjectiveWorkbenchRow) {
  if (!row.observed_answer_json) {
    return row.observation_state === "blank" ? "空白" : "—";
  }
  try {
    const value = JSON.parse(row.observed_answer_json) as {
      selected?: boolean;
      selected_labels?: string[];
      selected_values?: boolean[];
    };
    if (typeof value.selected === "boolean") {
      return value.selected ? "正确（√）" : "错误（×）";
    }
    if (Array.isArray(value.selected_labels)) {
      return value.selected_labels.join("、") || "—";
    }
    if (Array.isArray(value.selected_values)) {
      return value.selected_values.map((selected) => selected ? "√" : "×").join("、") + "（冲突）";
    }
  } catch {
    return row.observed_answer_json;
  }
  return row.observed_answer_json;
}

function objectiveOutcomeLabel(row: ObjectiveWorkbenchRow) {
  if (row.suggestion_outcome === "correct") return "建议正确";
  if (row.suggestion_outcome === "incorrect") return "建议错误";
  return "无法计分";
}

function ObjectiveReviewTab({
  workbench,
  taskMode=false,
  onDone,
  onError,
}: {
  taskMode?:boolean;
  workbench: ObjectiveWorkbench;
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
      eligibleCount,
      exceptionCount,
      busy,
      manualScores,
      manualNotes,
    },
    actions: {
      selectAssessment,
      selectItem,
      changeManualScore,
      changeManualNote,
      acceptOne,
      correctOne,
      acceptStrictBatch,
      retryRecognition,
      publishAttempt,
    },
  } = useObjectiveReviewController(workbench, onDone, onError);

  if (workbench.rows.length === 0) {
    return (
      <div className="empty-state objective-empty">
        <b>还没有可终审的标准卷客观题。</b>
        <span>真实视觉识别只处理已完成学生匹配、页面配准、答案区域和答题格确认的数据；上传后仍有异常时会先留给老师确认。</span>
        <span>需要临时录入时可继续使用“快速批改”兜底。</span>
      </div>
    );
  }

  return (
    <>
      {taskMode ? <section className="task-question"><b>第 {rows[0]?.question_no} 题 · {rows[0]?.question_stem}</b>
        {eligibleCount > 1 && <div><button className="primary" disabled={busy} onClick={()=>void acceptStrictBatch()}>确认本题 {eligibleCount} 条符合条件的结果</button><p className="muted">仅纳入满足原严格批量规则的项目；异常项仍需逐条核对。</p></div>}
      </section> : <section className="objective-toolbar exam-card">
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
                第 {row.order_index} 题 · {OBJECTIVE_TYPE_LABEL[row.question_type]} · {row.question_stem.slice(0, 34)}
              </option>
            ))}
          </select>
        </label>
        <div className="objective-stats">
          <span><b>{rows.length}</b> 份作答</span>
          <span className="ok-text"><b>{confirmedCount}</b> 已确认</span>
          <span><b>{eligibleCount}</b> 可严格批量</span>
          <span className={exceptionCount ? "bad-text" : ""}><b>{exceptionCount}</b> 需单独处理</span>
        </div>
        <button className="primary" disabled={busy || eligibleCount === 0} onClick={acceptStrictBatch}>
          {busy ? "处理中…" : `确认 ${eligibleCount} 条高置信度结果`}
        </button>
        <div className="meta objective-batch-note">阈值固定为 0.95；空白、涂改、低置信度、识别失败和已终审记录都会明确排除。</div>
      </section>}

      <div className="sech">本题证据 <span className="n">按学号排序</span></div>
      <div className="objective-review-list">
        {rows.map((row) => {
          const canAccept = row.suggested_score != null && !row.current_suggestion_confirmed;
          return (
            <article className={row.current_suggestion_confirmed ? "objective-review-row confirmed" : "objective-review-row"} key={row.suggestion_id}>
              <div className="objective-student">
                <b>{row.student_name}</b>
                <span>{row.student_no}</span>
                <span className={row.current_suggestion_confirmed ? "tag pass" : row.batch_eligible ? "tag wait" : "tag fail"}>
                  {row.current_suggestion_confirmed ? "已终审" : row.batch_eligible ? "可严格批量" : "需单独处理"}
                </span>
              </div>
              <div className="objective-evidence">
                {row.crop_path ? (
                  <img className="objective-crop" src={convertFileSrc(row.crop_path)} alt={`${row.student_name} 第 ${row.order_index} 题答案区域`} />
                ) : (
                  <div className="objective-crop missing">无答案裁剪</div>
                )}
                <div className="objective-facts">
                  <span>观察结果 <b>{OBSERVATION_LABEL[row.observation_state] ?? row.observation_state}</b></span>
                  <span>识别答案 <b>{displayObservedAnswer(row)}</b></span>
                  <span>置信度 <b>{row.confidence == null ? "—" : `${(row.confidence * 100).toFixed(1)}%`}</b></span>
                  <span>机器建议 <b className={row.suggestion_outcome === "correct" ? "ok-text" : row.suggestion_outcome === "incorrect" ? "bad-text" : ""}>{objectiveOutcomeLabel(row)}</b></span>
                  <span>建议得分 <b>{row.suggested_score == null ? "—" : `${row.suggested_score} / ${row.max_score}`}</b></span>
                  {row.exclusion_reason && <span>排除原因 <b>{EXCLUSION_LABEL[row.exclusion_reason] ?? row.exclusion_reason}</b></span>}
                  {row.current_suggestion_confirmed && <span>老师终审 <b>{row.teacher_score} 分 · {row.review_mode === "strict_batch" ? "严格批量" : "逐条确认"}</b></span>}
                </div>
              </div>
              <div className="objective-actions">
                {row.observation_state === "failed" && !row.current_suggestion_confirmed && (
                  <button disabled={busy} onClick={() => retryRecognition(row)}>重新整理本题</button>
                )}
                <button className={canAccept ? "primary" : ""} disabled={busy || !canAccept} onClick={() => acceptOne(row)}>
                  {row.current_suggestion_confirmed ? "已确认" : row.suggested_score == null ? "机器无法计分" : "接受本条建议"}
                </button>
                {!row.current_suggestion_confirmed && (
                  <details>
                    <summary>人工记分</summary>
                    <label className="field">
                      <span className="fl">得分（满分 {row.max_score}）</span>
                      <input
                        type="number"
                        min="0"
                        max={row.max_score}
                        step="0.5"
                        value={manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "")}
                        onChange={(event) => changeManualScore(row.suggestion_id, event.target.value)}
                      />
                    </label>
                    <label className="field">
                      <span className="fl">证据依据</span>
                      <input
                        type="text"
                        placeholder="如：查看原图后确认涂改答案"
                        value={manualNotes[row.suggestion_id] ?? ""}
                        onChange={(event) => changeManualNote(row.suggestion_id, event.target.value)}
                      />
                    </label>
                    <button disabled={busy} onClick={() => correctOne(row)}>保存人工 revision</button>
                  </details>
                )}
              </div>
            </article>
          );
        })}
      </div>

      {!taskMode && <><div className="sech">整卷发布 <span className="n">终审完成不等于已发布</span></div>
      <div className="objective-publish-grid">
        {attempts.map((attempt) => (
          <article className="exam-card objective-attempt" key={attempt.attempt_id}>
            <div className="exam-card-head">
              <div><b>{attempt.student_name}</b><div className="meta">{attempt.student_no}</div></div>
              <span className={attempt.attempt_state === "published" ? "tag pass" : attempt.can_publish ? "tag wait" : "tag"}>
                {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "待发布" : "终审中"}
              </span>
            </div>
            <div className="objective-attempt-score">
              <b>{attempt.teacher_total_score}</b><span>/ {attempt.max_total_score} 分</span>
            </div>
            <div className="meta">已终审 {attempt.confirmed_count} / {attempt.item_count} 题</div>
            {attempt.published_total_score != null && <div className="meta">当前已发布总分：{attempt.published_total_score}</div>}
            <button className="primary" disabled={busy || !attempt.can_publish} onClick={() => publishAttempt(attempt.attempt_id, attempt.student_name)}>
              {attempt.attempt_state === "published" ? "已发布" : attempt.can_publish ? "确认发布整卷" : "完成全部终审后发布"}
            </button>
          </article>
        ))}
      </div></>}
    </>
  );
}

export { ObjectiveReviewTab };
