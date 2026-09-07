import { useEffect, useMemo, useState } from "react";
import {
  SubjectiveWorkbench,
  SubjectiveWorkbenchRow,
  examAnswerSheetCorrectSubjectiveTranscription,
  examAnswerSheetGradeShortAnswer,
  examAnswerSheetPromoteAcceptedAnswer,
  examAnswerSheetPromoteRubricEvidence,
  examAnswerSheetRecognizeSubjectiveRegion,
  examAnswerSheetSubjectiveAccept,
  examAnswerSheetSubjectiveCorrect,
  examAnswerSheetSubjectiveCorrectComponents,
  examAnswerSheetSubjectivePublishAttempt,
} from "../../api/exam";
import { fillAnswerSlots, parseRequiredScore, shortAnswerRubricPoints } from "./examPure";

type TeacherComponentResult = {
  sourcePublicId: string;
  stableId: string;
  evidenceText: string;
};

function shortAnswerPointResults(raw: string | null) {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!Array.isArray(value.point_results)) return [];
    return value.point_results.map((rawPoint) => {
      const point = rawPoint as Record<string, unknown>;
      return {
        stableId: typeof point.stable_id === "string" ? point.stable_id : "unknown",
        status: typeof point.status === "string" ? point.status : "uncertain",
        suggestedScore: typeof point.suggested_score === "number" ? point.suggested_score : 0,
        evidenceSnippets: Array.isArray(point.evidence_snippets)
          ? point.evidence_snippets.filter((item): item is string => typeof item === "string")
          : [],
        reason: typeof point.reason === "string" ? point.reason : "",
      };
    });
  } catch {
    return [];
  }
}

function useSubjectiveReviewController(
  workbench: SubjectiveWorkbench,
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
  const [componentScores, setComponentScores] = useState<Record<string, string>>({});
  const [componentEvidence, setComponentEvidence] = useState<Record<string, string>>({});
  const [componentNotes, setComponentNotes] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!assessmentVersions.some((version) => version.id === assessmentVersionId)) {
      setAssessmentVersionId(assessmentVersions[0]?.id ?? 0);
    }
  }, [assessmentVersionId, assessmentVersions]);

  const itemOptions = useMemo(() => {
    const unique = new Map<number, SubjectiveWorkbenchRow>();
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
  const directCount = rows.filter((row) => !row.current_suggestion_confirmed && row.suggested_score != null).length;
  const exceptionCount = rows.length - confirmedCount - directCount;

  const selectAssessment = (value: number) => setAssessmentVersionId(value);
  const selectItem = (value: number) => setAssessmentItemId(value);
  const changeCorrection = (answerRegionRevisionId: number, value: string) => {
    setCorrections((current) => ({ ...current, [answerRegionRevisionId]: value }));
  };
  const changeManualScore = (suggestionId: number, value: string) => {
    setManualScores((current) => ({ ...current, [suggestionId]: value }));
  };
  const changeManualNote = (suggestionId: number, value: string) => {
    setManualNotes((current) => ({ ...current, [suggestionId]: value }));
  };
  const changeComponentScore = (key: string, value: string) => {
    setComponentScores((current) => ({ ...current, [key]: value }));
  };
  const changeComponentEvidence = (key: string, value: string) => {
    setComponentEvidence((current) => ({ ...current, [key]: value }));
  };
  const changeComponentNote = (key: string, value: string) => {
    setComponentNotes((current) => ({ ...current, [key]: value }));
  };

  async function correctTranscription(row: SubjectiveWorkbenchRow) {
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
      const revision = await examAnswerSheetCorrectSubjectiveTranscription(row.answer_region_revision_id, value);
      let gradeMessage = "";
      if (row.question_type === "short_answer") {
        try {
          await examAnswerSheetGradeShortAnswer(
            revision.id,
            `answer-sheet:transcription:${revision.id}:answer-grade:${crypto.randomUUID()}`,
          );
          gradeMessage = "；已按新文本生成逐点评分建议";
        } catch (gradeError) {
          gradeMessage = `；评分建议暂未生成（${String(gradeError)}）`;
        }
      }
      onDone(`${row.student_name}第${row.question_no}题已保存老师校正文本；机器原文仍保留${gradeMessage}`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function retry(row: SubjectiveWorkbenchRow) {
    setBusy(true);
    try {
      const revision = await examAnswerSheetRecognizeSubjectiveRegion(
        row.answer_region_revision_id,
        `answer-sheet:subjective:${row.answer_region_revision_id}:retry:${crypto.randomUUID()}`,
      );
      let gradeMessage = "";
      if (revision.question_type === "short_answer" && revision.result_state === "recognized") {
        try {
          await examAnswerSheetGradeShortAnswer(
            revision.id,
            `answer-sheet:transcription:${revision.id}:answer-grade:${crypto.randomUUID()}`,
          );
          gradeMessage = "；已生成逐点评分建议";
        } catch (gradeError) {
          gradeMessage = `；评分建议暂未生成（${String(gradeError)}）`;
        }
      }
      onDone(`${row.student_no}号第${row.question_no}题已重新识别；旧转写仍保留${gradeMessage}`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function generateShortAnswerGrade(row: SubjectiveWorkbenchRow) {
    setBusy(true);
    try {
      await examAnswerSheetGradeShortAnswer(
        row.transcription_revision_id,
        `answer-sheet:transcription:${row.transcription_revision_id}:answer-grade:${crypto.randomUUID()}`,
      );
      onDone(`${row.student_name}第${row.question_no}题已生成逐评分点建议，等待老师终审`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function accept(row: SubjectiveWorkbenchRow) {
    setBusy(true);
    try {
      await examAnswerSheetSubjectiveAccept(row.suggestion_id);
      onDone(`已终审 ${row.student_name} 的第${row.question_no}题；整份答题卡仍需显式发布`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function correctGrade(row: SubjectiveWorkbenchRow) {
    const score = parseRequiredScore(manualScores[row.suggestion_id] ?? String(row.suggested_score ?? ""));
    const note = (manualNotes[row.suggestion_id] ?? "").trim();
    if (!Number.isFinite(score) || score < 0 || score > row.max_score) {
      onError(`人工得分必须位于 0~${row.max_score} 分`);
      return;
    }
    if (!note) {
      onError("人工记分必须填写查看原图或评分点后的判定依据");
      return;
    }
    setBusy(true);
    try {
      await examAnswerSheetSubjectiveCorrect(row.suggestion_id, score, note);
      onDone(`已人工确认 ${row.student_name} 的第${row.question_no}题为 ${score} 分`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function correctComponents(row: SubjectiveWorkbenchRow) {
    const note = (manualNotes[row.suggestion_id] ?? "").trim();
    if (!note) {
      onError("逐项终审仍需填写本题整体判定依据");
      return;
    }
    const pointResults = shortAnswerPointResults(row.suggestion_result_json);
    const specs = row.question_type === "fill_blank"
      ? fillAnswerSlots(row.answer_slots_json).map((slot) => ({ ...slot, sourceType: "answer_slot" as const }))
      : shortAnswerRubricPoints(row.rubric_points_json).map((point) => ({
        ...point,
        sourceType: "rubric_point" as const,
      }));
    if (!specs.length || specs.some((spec) => !spec.sourcePublicId || spec.maxScore <= 0)) {
      onError("当前答案版本缺少完整槽位或评分点，请先修正答案规则");
      return;
    }
    const components = specs.map((spec) => {
      const key = `${row.suggestion_id}:${spec.sourcePublicId}`;
      const machinePoint = pointResults.find((point) => point.stableId === spec.stableId);
      const defaultScore = row.question_type === "short_answer"
        ? machinePoint?.suggestedScore
        : specs.length === 1
          ? row.suggested_score
          : undefined;
      const defaultEvidence = row.question_type === "short_answer"
        ? machinePoint?.evidenceSnippets[0] ?? ""
        : specs.length === 1
          ? row.teacher_corrected_text ?? row.normalized_text ?? row.raw_ocr_text ?? ""
          : "";
      return {
        source_type: spec.sourceType,
        source_public_id: spec.sourcePublicId,
        teacher_score: parseRequiredScore(componentScores[key] ?? String(defaultScore ?? "")),
        evidence_text: (componentEvidence[key] ?? defaultEvidence).trim() || null,
        teacher_note: (componentNotes[key] ?? "").trim() || null,
        label: spec.canonicalText,
        maxScore: spec.maxScore,
      };
    });
    const invalid = components.find((component) => (
      !Number.isFinite(component.teacher_score)
      || component.teacher_score < 0
      || component.teacher_score > component.maxScore
      || (component.teacher_score > 0 && !component.evidence_text)
    ));
    if (invalid) {
      onError(`${invalid.label}：得分必须位于 0~${invalid.maxScore}，给分时必须填写学生作答证据`);
      return;
    }
    setBusy(true);
    try {
      const decision = await examAnswerSheetSubjectiveCorrectComponents(
        row.suggestion_id,
        components.map((component) => ({
          source_type: component.source_type,
          source_public_id: component.source_public_id,
          teacher_score: component.teacher_score,
          evidence_text: component.evidence_text,
          teacher_note: component.teacher_note,
        })),
        note,
      );
      onDone(`已逐项确认 ${row.student_name} 的第${row.question_no}题，自动汇总为 ${decision.teacher_score} 分`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function promoteAcceptedAnswer(row: SubjectiveWorkbenchRow) {
    if (row.grade_decision_id == null) {
      onError("请先完成本题人工终审");
      return;
    }
    const acceptedText = (row.teacher_corrected_text ?? row.normalized_text ?? "").trim();
    if (!acceptedText) {
      onError("当前没有可加入答案库的老师确认写法");
      return;
    }
    if (!window.confirm(
      `确认把“${acceptedText}”加入未来可接受答案？\n\n系统会创建新的答案与作业版本；本次得分、已发布成绩和历史记录均不会改变。`,
    )) return;
    setBusy(true);
    try {
      const result = await examAnswerSheetPromoteAcceptedAnswer(row.grade_decision_id);
      const prefix = result.outcome === "created_new_version"
        ? "已创建新答案版本"
        : result.outcome === "already_promoted"
          ? "该写法此前已加入答案库"
          : "最新答案版本已包含该写法";
      onDone(`${prefix}：${result.accepted_text}；当前作业与历史成绩保持不变`);
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function promoteRubricEvidence(row: SubjectiveWorkbenchRow, component: TeacherComponentResult) {
    if (row.grade_decision_id == null || !component.evidenceText.trim()) {
      onError("请先逐评分点确认得分并引用学生作答原文");
      return;
    }
    if (!window.confirm(
      `确认把“${component.evidenceText}”加入“${component.stableId}”的未来评分点示例？\n\n系统会创建新的评分规则、链接集与作业版本；本次得分、已发布成绩和历史记录均不会改变。`,
    )) return;
    setBusy(true);
    try {
      const result = await examAnswerSheetPromoteRubricEvidence(
        row.grade_decision_id,
        component.sourcePublicId,
      );
      const prefix = result.outcome === "created_new_version"
        ? "已创建新评分规则"
        : result.outcome === "already_promoted"
          ? "该表述此前已加入评分规则"
          : "最新评分规则已包含该表述";
      onDone(
        `${prefix}：${result.evidence_text}；沿用 ${result.carried_knowledge_link_count} 条知识链接和 ${result.carried_ability_link_count} 条能力链接，当前成绩保持不变`,
      );
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function publishAttempt(attemptId: number, studentName: string) {
    if (!window.confirm(`确认发布 ${studentName} 的本次答题卡成绩？只采用当前老师终审 revision。`)) return;
    setBusy(true);
    try {
      const publication = await examAnswerSheetSubjectivePublishAttempt(attemptId);
      onDone(`${studentName} 的答题卡成绩已发布：${publication.total_score} 分（revision ${publication.revision}）`);
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
  };
}

export { shortAnswerPointResults, useSubjectiveReviewController };
