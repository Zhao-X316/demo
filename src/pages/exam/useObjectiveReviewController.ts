import { useEffect, useMemo, useState } from "react";
import {
  ObjectiveWorkbench,
  ObjectiveWorkbenchRow,
  examObjectiveAccept,
  examObjectiveCorrect,
  examObjectivePublishAttempt,
  examObjectiveRecognizeRegion,
  examObjectiveStrictBatchAccept,
} from "../../api/exam";
import { parseRequiredScore } from "./examPure";

function useObjectiveReviewController(
  workbench: ObjectiveWorkbench,
  onDone: (message: string) => void,
  onError: (message: string) => void,
) {
  const assessmentVersions = useMemo(() => {
    const unique = new Map<number, string>();
    workbench.rows.forEach((row) => unique.set(row.assessment_version_id, row.assessment_title));
    return Array.from(unique, ([id, title]) => ({ id, title }));
  }, [workbench.rows]);
  const [assessmentVersionId, setAssessmentVersionId] = useState(assessmentVersions[0]?.id ?? 0);
  const [assessmentItemId, setAssessmentItemId] = useState(0);
  const [busy, setBusy] = useState(false);
  const [manualScores, setManualScores] = useState<Record<number, string>>({});
  const [manualNotes, setManualNotes] = useState<Record<number, string>>({});

  useEffect(() => {
    if (!assessmentVersions.some((version) => version.id === assessmentVersionId)) {
      setAssessmentVersionId(assessmentVersions[0]?.id ?? 0);
    }
  }, [assessmentVersionId, assessmentVersions]);

  const itemOptions = useMemo(() => {
    const unique = new Map<number, ObjectiveWorkbenchRow>();
    workbench.rows
      .filter((row) => row.assessment_version_id === assessmentVersionId)
      .forEach((row) => unique.set(row.assessment_item_id, row));
    return Array.from(unique.values()).sort((left, right) => left.order_index - right.order_index);
  }, [assessmentVersionId, workbench.rows]);

  useEffect(() => {
    if (!itemOptions.some((row) => row.assessment_item_id === assessmentItemId)) {
      setAssessmentItemId(itemOptions[0]?.assessment_item_id ?? 0);
    }
  }, [assessmentItemId, itemOptions]);

  const rows = workbench.rows
    .filter((row) => row.assessment_version_id === assessmentVersionId && row.assessment_item_id === assessmentItemId)
    .sort((left, right) => left.student_no.localeCompare(right.student_no, "zh-CN", { numeric: true }));
  const attempts = workbench.attempts.filter((attempt) => attempt.assessment_version_id === assessmentVersionId);
  const confirmedCount = rows.filter((row) => row.current_suggestion_confirmed).length;
  const eligibleCount = rows.filter((row) => row.batch_eligible && !row.current_suggestion_confirmed).length;
  const exceptionCount = rows.length - confirmedCount - eligibleCount;

  const selectAssessment = (value: number) => setAssessmentVersionId(value);
  const selectItem = (value: number) => setAssessmentItemId(value);
  const changeManualScore = (suggestionId: number, value: string) => {
    setManualScores((current) => ({ ...current, [suggestionId]: value }));
  };
  const changeManualNote = (suggestionId: number, value: string) => {
    setManualNotes((current) => ({ ...current, [suggestionId]: value }));
  };

  const acceptOne = async (row: ObjectiveWorkbenchRow) => {
    setBusy(true);
    try {
      await examObjectiveAccept(row.suggestion_id);
      onDone(`已确认 ${row.student_name} 的第 ${row.order_index} 题，成绩仍需整卷显式发布`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const correctOne = async (row: ObjectiveWorkbenchRow) => {
    const rawScore = manualScores[row.suggestion_id] ?? String(row.suggested_score ?? "");
    const score = parseRequiredScore(rawScore);
    const note = (manualNotes[row.suggestion_id] ?? "").trim();
    if (!Number.isFinite(score) || score < 0 || score > row.max_score) {
      onError(`人工得分必须位于 0~${row.max_score} 分`);
      return;
    }
    if (!note) {
      onError("人工记分必须填写查看原图后的证据依据");
      return;
    }
    setBusy(true);
    try {
      await examObjectiveCorrect(row.suggestion_id, score, note);
      onDone(`已人工确认 ${row.student_name} 的第 ${row.order_index} 题为 ${score} 分`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const acceptStrictBatch = async () => {
    if (eligibleCount === 0) return;
    setBusy(true);
    try {
      const batch = await examObjectiveStrictBatchAccept(
        rows.map((row) => row.suggestion_id),
        `objective-review-${crypto.randomUUID()}`,
      );
      onDone(`严格批量终审完成：确认 ${batch.confirmed_count} 条，排除 ${batch.excluded_count} 条异常`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const retryRecognition = async (row: ObjectiveWorkbenchRow) => {
    setBusy(true);
    try {
      await examObjectiveRecognizeRegion(
        row.answer_region_revision_id,
        `objective-omr-${row.answer_region_revision_id}-${crypto.randomUUID()}`,
      );
      onDone(`已重新整理 ${row.student_name} 的第 ${row.order_index} 题；机器结果仍需老师确认`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const publishAttempt = async (attemptId: number, studentName: string) => {
    if (!window.confirm(`确认发布 ${studentName} 的本次整卷成绩？发布只采用当前老师终审 revision。`)) return;
    setBusy(true);
    try {
      const publication = await examObjectivePublishAttempt(attemptId);
      onDone(`${studentName} 的成绩已发布：${publication.total_score} 分（发布 revision ${publication.revision}）`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return {
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
  };
}

export { useObjectiveReviewController };
