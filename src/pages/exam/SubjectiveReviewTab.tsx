import {
  SubjectiveWorkbench,
} from "../../api/exam";
import { fillAnswerSlots, shortAnswerRubricPoints } from "./examPure";
import { SubjectiveAttemptPublishPanel } from "./SubjectiveAttemptPublishPanel";
import { SubjectiveComponentEditor } from "./SubjectiveComponentEditor";
import { SubjectiveEvidenceSummary } from "./SubjectiveEvidenceSummary";
import { SubjectiveLinkPanel } from "./SubjectiveLinkPanel";
import { SubjectiveReviewToolbar } from "./SubjectiveReviewToolbar";
import {
  shortAnswerPointResults,
  useSubjectiveReviewController,
} from "./useSubjectiveReviewController";

function teacherComponentResults(raw: string | null) {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!Array.isArray(value.component_results)) return [];
    return value.component_results.flatMap((rawComponent) => {
      if (!rawComponent || typeof rawComponent !== "object" || Array.isArray(rawComponent)) return [];
      const component = rawComponent as Record<string, unknown>;
      if (component.source_type !== "answer_slot" && component.source_type !== "rubric_point") return [];
      return [{
        sourceType: component.source_type,
        sourcePublicId: typeof component.source_public_id === "string" ? component.source_public_id : "",
        stableId: typeof component.stable_id === "string" ? component.stable_id : "unknown",
        teacherScore: typeof component.teacher_score === "number" ? component.teacher_score : 0,
        maxScore: typeof component.max_score === "number" ? component.max_score : 0,
        resultStatus: typeof component.result_status === "string" ? component.result_status : "incorrect",
        evidenceText: typeof component.evidence_text === "string" ? component.evidence_text : "",
        teacherNote: typeof component.teacher_note === "string" ? component.teacher_note : "",
      }];
    });
  } catch {
    return [];
  }
}

function rubricEvidencePromotions(raw: string | null) {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!Array.isArray(value.promotions)) return [];
    return value.promotions.flatMap((rawPromotion) => {
      if (!rawPromotion || typeof rawPromotion !== "object" || Array.isArray(rawPromotion)) return [];
      const promotion = rawPromotion as Record<string, unknown>;
      return [{
        promotionId: typeof promotion.promotion_id === "number" ? promotion.promotion_id : 0,
        sourcePublicId: typeof promotion.source_public_id === "string" ? promotion.source_public_id : "",
        stableId: typeof promotion.rubric_point_stable_id === "string"
          ? promotion.rubric_point_stable_id
          : "",
        evidenceText: typeof promotion.evidence_text === "string" ? promotion.evidence_text : "",
        adoptedAssessmentVersionId: typeof promotion.adopted_assessment_version_id === "number"
          ? promotion.adopted_assessment_version_id
          : 0,
      }];
    });
  } catch {
    return [];
  }
}
function SubjectiveReviewTab({
  workbench,
  taskMode=false,
  onDone,
  onError,
}: {
  taskMode?:boolean;
  workbench: SubjectiveWorkbench;
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
      directCount,
      exceptionCount,
      busy,
      corrections,
      manualScores,
      manualNotes,
      componentScores,
      componentEvidence,
      componentNotes,
    },
    actions: {
      selectAssessment,
      selectItem,
      changeCorrection,
      changeManualScore,
      changeManualNote,
      changeComponentScore,
      changeComponentEvidence,
      changeComponentNote,
      correctTranscription,
      retry,
      generateShortAnswerGrade,
      accept,
      correctGrade,
      correctComponents,
      promoteAcceptedAnswer,
      promoteRubricEvidence,
      publishAttempt,
    },
  } = useSubjectiveReviewController(workbench, onDone, onError);

  if (!workbench.rows.length) {
    return (
      <div className="empty-state objective-empty">
        <b>还没有答题卡主观题转写。</b>
        <span>上传并处理答题卡后，填空题会做确定性答案比对；篇幅受控简答题会自动按评分点整理原文证据。</span>
      </div>
    );
  }

  return (
    <>
      {taskMode ? <section className="task-question"><b>第 {rows[0]?.question_no} 题 · {rows[0]?.question_stem}</b></section> : <SubjectiveReviewToolbar
        assessmentVersions={assessmentVersions}
        assessmentVersionId={assessmentVersionId}
        onAssessmentVersionChange={selectAssessment}
        itemOptions={itemOptions}
        assessmentItemId={assessmentItemId}
        onAssessmentItemChange={selectItem}
        rowCount={rows.length}
        confirmedCount={confirmedCount}
        directCount={directCount}
        exceptionCount={exceptionCount}
      />}

      <details open={!taskMode}><summary>知识与能力链接</summary>
      <SubjectiveLinkPanel
        assessmentItemId={assessmentItemId}
        onDone={onDone}
        onError={onError}
      /></details>

      <div className="sech">本题证据 <span className="n">按学号排序</span></div>
      <div className="objective-review-list">
        {rows.map((row) => {
          const value = corrections[row.answer_region_revision_id]
            ?? row.teacher_corrected_text
            ?? row.raw_ocr_text
            ?? "";
          const rubricPoints = shortAnswerRubricPoints(row.rubric_points_json);
          const answerSlots = fillAnswerSlots(row.answer_slots_json);
          const pointResults = row.short_answer_analysis_id == null
            ? []
            : shortAnswerPointResults(row.suggestion_result_json);
          const componentSpecs = row.question_type === "fill_blank"
            ? answerSlots.map((slot) => ({ ...slot, sourceType: "answer_slot" as const }))
            : rubricPoints.map((point) => ({ ...point, sourceType: "rubric_point" as const }));
          const confirmedComponents = teacherComponentResults(row.teacher_components_json);
          const promotedRubricEvidence = rubricEvidencePromotions(row.rubric_evidence_promotions_json);
          return (
            <article className={row.current_suggestion_confirmed ? "objective-review-row confirmed" : "objective-review-row"} key={row.suggestion_id}>
              <SubjectiveEvidenceSummary
                row={row}
                rubricPoints={rubricPoints}
                pointResults={pointResults}
                promotedRubricEvidence={promotedRubricEvidence}
              />
              {confirmedComponents.length > 0 && (
                <div className="short-answer-analysis teacher-components">
                  {confirmedComponents.map((component) => {
                    const promotion = promotedRubricEvidence.find(
                      (item) => item.sourcePublicId === component.sourcePublicId,
                    );
                    return (
                      <div className={`short-answer-point ${component.resultStatus}`} key={component.sourcePublicId}>
                        <div>
                          <b>{componentSpecs.find((item) => item.sourcePublicId === component.sourcePublicId)?.canonicalText || component.stableId}</b>
                          <span>老师逐项确认 · {component.teacherScore} / {component.maxScore} 分</span>
                        </div>
                        {component.evidenceText
                          ? <q>{component.evidenceText}</q>
                          : <em>本项未给分，无需填写作答证据</em>}
                        {component.teacherNote && <p>{component.teacherNote}</p>}
                        {row.question_type === "short_answer"
                          && component.teacherScore > 0
                          && component.evidenceText
                          && (promotion
                            ? <span className="tag pass">已加入未来评分规则</span>
                            : (
                              <button disabled={busy} onClick={() => void promoteRubricEvidence(row, component)}>
                                加入未来评分点示例
                              </button>
                            ))}
                      </div>
                    );
                  })}
                </div>
              )}
              <div className="objective-actions">
                {!row.current_suggestion_confirmed
                  && row.question_type === "short_answer"
                  && row.result_state === "recognized"
                  && row.short_answer_analysis_id == null && (
                    <button disabled={busy} onClick={() => void generateShortAnswerGrade(row)}>生成逐点评分建议</button>
                  )}
                {!row.current_suggestion_confirmed && row.suggested_score != null && (
                  <button className="primary" disabled={busy} onClick={() => void accept(row)}>接受本条建议</button>
                )}
                {!row.current_suggestion_confirmed && row.result_state === "recognized" && (
                  <details>
                    <summary>校正机器原文</summary>
                    <input
                      value={value}
                      aria-label={`${row.student_name} 第${row.question_no}题实际书写`}
                      onChange={(event) => changeCorrection(row.answer_region_revision_id, event.target.value)}
                    />
                    <button disabled={busy} onClick={() => void correctTranscription(row)}>按原图保存实际书写</button>
                  </details>
                )}
                {!row.current_suggestion_confirmed && row.result_state === "recognize_failed" && (
                  <button disabled={busy} onClick={() => void retry(row)}>重新识别本题</button>
                )}
                <SubjectiveComponentEditor
                  row={row}
                  componentSpecs={componentSpecs}
                  pointResults={pointResults}
                  componentScores={componentScores}
                  changeComponentScore={changeComponentScore}
                  componentEvidence={componentEvidence}
                  changeComponentEvidence={changeComponentEvidence}
                  componentNotes={componentNotes}
                  changeComponentNote={changeComponentNote}
                  manualNotes={manualNotes}
                  changeManualNote={changeManualNote}
                  busy={busy}
                  correctComponents={correctComponents}
                />
                {!row.current_suggestion_confirmed && (
                  <details>
                    <summary>仅记整题总分（不形成逐项图谱证据）</summary>
                    <label className="field">
                      <span className="fl">得分（满分 {row.max_score}）</span>
                      <input type="number" min="0" max={row.max_score} step="0.5"
                        value={manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "")}
                        onChange={(event) => changeManualScore(row.suggestion_id, event.target.value)} />
                    </label>
                    <label className="field">
                      <span className="fl">判定依据（必填）</span>
                      <input type="text" placeholder="如：覆盖评分点1和2，缺少影响"
                        value={manualNotes[row.suggestion_id] ?? ""}
                        onChange={(event) => changeManualNote(row.suggestion_id, event.target.value)} />
                    </label>
                    <button disabled={busy} onClick={() => void correctGrade(row)}>保存人工 revision</button>
                  </details>
                )}
                {row.question_type === "fill_blank"
                  && answerSlots.length === 1
                  && row.current_suggestion_confirmed
                  && row.confirmation_level === "teacher_corrected"
                  && row.teacher_score != null
                  && Math.abs(row.teacher_score - row.max_score) < 0.000001
                  && row.suggestion_outcome !== "correct"
                  && row.accepted_answer_promotion_id == null && (
                    <button disabled={busy} onClick={() => void promoteAcceptedAnswer(row)}>
                      加入未来可接受答案
                    </button>
                  )}
              </div>
            </article>
          );
        })}
      </div>

      {!taskMode && <SubjectiveAttemptPublishPanel
        attempts={attempts}
        busy={busy}
        onPublish={publishAttempt}
      />}
    </>
  );
}

export { SubjectiveReviewTab };
