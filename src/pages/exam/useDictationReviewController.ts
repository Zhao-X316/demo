import { useEffect, useMemo, useState } from "react";
import {
  DictationWorkbench,
  DictationWorkbenchRow,
  examDictationAccept,
  examDictationCorrectGrade,
  examDictationCorrectTranscription,
  examDictationPublishAttempt,
  examDictationRecognizeRegion,
  examDictationStrictBatchAccept,
} from "../../api/exam";
import { parseRequiredScore } from "./examPure";

function useDictationReviewController(
  workbench: DictationWorkbench,
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
  const [corrections, setCorrections] = useState<Record<number, string>>({});
  const [manualScores, setManualScores] = useState<Record<number, string>>({});
  const [manualNotes, setManualNotes] = useState<Record<number, string>>({});
  const [manualEvidence, setManualEvidence] = useState<Record<number, string>>({});

  useEffect(() => {
    if (!assessmentVersions.some((version) => version.id === assessmentVersionId)) {
      setAssessmentVersionId(assessmentVersions[0]?.id ?? 0);
    }
  }, [assessmentVersionId, assessmentVersions]);

  const itemOptions = useMemo(() => {
    const unique = new Map<number, DictationWorkbenchRow>();
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
  const confirmedCount = rows.filter((row) => row.current_transcription_confirmed).length;
  const eligibleRows = rows.filter((row) => (
    !row.current_transcription_confirmed
    && !row.requires_teacher_review
    && row.suggested_score != null
    && row.result_state === "recognized"
    && row.confidence != null
    && row.confidence >= 0.95
  ));
  const exceptionCount = rows.length - confirmedCount - eligibleRows.length;

  const selectAssessment = (value: number) => setAssessmentVersionId(value);
  const selectItem = (value: number) => setAssessmentItemId(value);
  const changeCorrection = (answerRegionRevisionId: number, value: string) => {
    setCorrections((current) => ({ ...current, [answerRegionRevisionId]: value }));
  };
  const changeManualScore = (transcriptionRevisionId: number, value: string) => {
    setManualScores((current) => ({ ...current, [transcriptionRevisionId]: value }));
  };
  const changeManualNote = (transcriptionRevisionId: number, value: string) => {
    setManualNotes((current) => ({ ...current, [transcriptionRevisionId]: value }));
  };
  const changeManualEvidence = (transcriptionRevisionId: number, value: string) => {
    setManualEvidence((current) => ({ ...current, [transcriptionRevisionId]: value }));
  };

  async function correct(row: DictationWorkbenchRow) {
    const value = (corrections[row.answer_region_revision_id]
      ?? row.teacher_corrected_text
      ?? row.raw_ocr_text
      ?? "").trim();
    if (!value) {
      onError("请先按原图填写学生实际写下的内容");
      return;
    }
    setBusy(true);
    try {
      await examDictationCorrectTranscription(row.answer_region_revision_id, value);
      onDone(`${row.student_name}第${row.question_no}题已保存实际书写；请再确认得分，机器原文仍保留`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function retry(row: DictationWorkbenchRow) {
    setBusy(true);
    try {
      await examDictationRecognizeRegion(
        row.answer_region_revision_id,
        `dictation:region:${row.answer_region_revision_id}:retry:${crypto.randomUUID()}`,
      );
      onDone(`${row.student_no}号第${row.question_no}题已重新识别；旧失败记录仍保留`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function acceptOne(row: DictationWorkbenchRow) {
    setBusy(true);
    try {
      await examDictationAccept(row.transcription_revision_id);
      onDone(`已终审 ${row.student_name} 的第${row.question_no}题；整份默写仍需显式发布`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function gradeOne(row: DictationWorkbenchRow) {
    const score = parseRequiredScore(manualScores[row.transcription_revision_id] ?? String(row.suggested_score ?? ""));
    const note = (manualNotes[row.transcription_revision_id] ?? "").trim();
    const evidence = (manualEvidence[row.transcription_revision_id] ?? row.teacher_corrected_text ?? "").trim();
    if (!Number.isFinite(score) || score < 0 || score > row.max_score) {
      onError(`人工得分必须位于 0~${row.max_score} 分`);
      return;
    }
    if (!note) {
      onError("人工记分必须填写查看原图后的判定依据");
      return;
    }
    setBusy(true);
    try {
      await examDictationCorrectGrade(
        row.transcription_revision_id,
        score,
        note,
        evidence || null,
      );
      onDone(`已人工确认 ${row.student_name} 的第${row.question_no}题为 ${score} 分`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function acceptStrictBatch() {
    if (!eligibleRows.length) return;
    setBusy(true);
    try {
      const batch = await examDictationStrictBatchAccept(
        rows.map((row) => row.transcription_revision_id),
        `dictation-review-${crypto.randomUUID()}`,
      );
      onDone(`严格批量终审完成：确认 ${batch.confirmed_count} 条，排除 ${batch.excluded_count} 条分歧`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function publishAttempt(attemptId: number, studentName: string) {
    if (!window.confirm(`确认发布 ${studentName} 的本次默写成绩？发布只采用当前老师终审 revision。`)) return;
    setBusy(true);
    try {
      const publication = await examDictationPublishAttempt(attemptId);
      onDone(`${studentName} 的默写成绩已发布：${publication.total_score} 分（revision ${publication.revision}）`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return {
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
  };
}

export { useDictationReviewController };
