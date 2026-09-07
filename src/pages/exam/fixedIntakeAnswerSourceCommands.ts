import {
  type RubricPointMappingInput,
  examAnswerSourceAdoptNewVersion,
  examAnswerSourceAnalyze,
  examAnswerSourceConfirmMatches,
  examAnswerSourceKeepBound,
} from "../../api/exam.ts";
import { shortAnswerRubricPoints } from "./examPure.ts";
import type { FixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import { fixedIntakeCompletionIdentity } from "./fixedIntakeRootState.ts";
import { NEW_RUBRIC_POINT } from "./fixedIntakeState.ts";
import type { FixedIntakeViewModel } from "./fixedIntakeViewModel.ts";

export function createFixedIntakeAnswerSourceCommands({
  runtime,
  view,
  onError,
}: {
  runtime: FixedIntakeControllerRuntime;
  view: FixedIntakeViewModel;
  onError: (message: string) => void;
}) {
  const {
    stateRef,
    dispatchAnswerSource,
    dispatchBatchWorkflow,
    completionIsCurrent,
    claimOperation,
  } = runtime;
  const { answerSourceAnalysis, result, rubricPointMappings } = view;

  async function analyzeAnswerSource(batchId: number, retry = false) {
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId,
    };
    const release = claimOperation(
      `answer-source:${batchId}:${retry ? "retry" : "initial"}`,
    );
    if (!release) return;
    dispatchAnswerSource({ type: "ANSWER_SOURCE_STARTED" });
    try {
      const analysis = await examAnswerSourceAnalyze(
        batchId,
        retry
          ? `answer-source:${batchId}:retry:${crypto.randomUUID()}`
          : `answer-source:${batchId}:structure:v3`,
      );
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_SUCCEEDED", analysis },
      });
      if (completionIsCurrent(identity)) {
        const reasonCode = analysis.run.status === "failed"
          ? "ANSWER_SOURCE_STRUCTURE_FAILED"
          : analysis.review?.route === "ready_to_confirm"
            ? "ANSWER_SOURCE_CONFIRMATION_REQUIRED"
            : analysis.review?.route === "blocked"
              ? "ANSWER_SOURCE_CONFLICT_OR_MISSING"
              : null;
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SOURCE_REASON_REPLACED", reasonCode },
        });
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        const message = String(err);
        dispatchAnswerSource({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SOURCE_FAILED", error: message },
        });
        onError(`答案资料暂未完成整理：${message}`);
      }
    } finally {
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_FINISHED" },
      });
      release();
    }
  }

  async function confirmMatchingAnswerSource() {
    const review = answerSourceAnalysis?.review;
    if (!result || !review) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
      runId: review.sourceAiRunId,
    };
    const release = claimOperation(
      `answer-resolution:${result.batchId}:${review.sourceAiRunId}`,
    );
    if (!release) return;
    dispatchAnswerSource({ type: "ANSWER_SOURCE_RESOLUTION_STARTED" });
    try {
      const confirmed = await examAnswerSourceConfirmMatches(
        result.batchId,
        review.sourceAiRunId,
      );
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED", review: confirmed },
      });
      if (completionIsCurrent(identity)) {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SOURCE_REASON_REPLACED", reasonCode: null },
        });
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`确认答案资料失败：${String(err)}`);
      }
    } finally {
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_FINISHED" },
      });
      release();
    }
  }

  async function keepCurrentBoundAnswers() {
    const review = answerSourceAnalysis?.review;
    if (!result || !review) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
      runId: review.sourceAiRunId,
    };
    const release = claimOperation(
      `answer-resolution:${result.batchId}:${review.sourceAiRunId}`,
    );
    if (!release) return;
    dispatchAnswerSource({ type: "ANSWER_SOURCE_RESOLUTION_STARTED" });
    try {
      const confirmed = await examAnswerSourceKeepBound(
        result.batchId,
        review.sourceAiRunId,
      );
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED", review: confirmed },
      });
      if (completionIsCurrent(identity)) {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SOURCE_REASON_REPLACED", reasonCode: null },
        });
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`沿用当前答案失败：${String(err)}`);
      }
    } finally {
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_FINISHED" },
      });
      release();
    }
  }

  async function adoptAnswerSourceAsNewVersion() {
    const review = answerSourceAnalysis?.review;
    if (!result || !review) return;
    const rubricMappings: RubricPointMappingInput[] = [];
    for (const item of review.items) {
      if (item.questionType !== "short_answer" || item.matchState !== "conflict") continue;
      const reused = new Set<string>();
      for (const point of shortAnswerRubricPoints(item.candidateAnswerJson)) {
        const selected = rubricPointMappings[`${item.assessmentItemId}:${point.orderIndex}`] || "";
        if (!selected) {
          onError(`第 ${item.questionNo} 题还有评分点没有确认对应关系`);
          return;
        }
        if (selected !== NEW_RUBRIC_POINT && !reused.add(selected)) {
          onError(`第 ${item.questionNo} 题不能把两个新评分点对应到同一个旧评分点`);
          return;
        }
        rubricMappings.push({
          assessmentItemId: item.assessmentItemId,
          candidateOrderIndex: point.orderIndex,
          action: selected === NEW_RUBRIC_POINT ? "new_point" : "reuse_existing",
          previousStableId: selected === NEW_RUBRIC_POINT ? null : selected,
        });
      }
    }
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
      runId: review.sourceAiRunId,
    };
    const release = claimOperation(
      `answer-resolution:${result.batchId}:${review.sourceAiRunId}`,
    );
    if (!release) return;
    dispatchAnswerSource({ type: "ANSWER_SOURCE_RESOLUTION_STARTED" });
    try {
      const confirmed = await examAnswerSourceAdoptNewVersion(
        result.batchId,
        review.sourceAiRunId,
        rubricMappings,
      );
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED", review: confirmed },
      });
      if (completionIsCurrent(identity)) {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SOURCE_REASON_REPLACED", reasonCode: null },
        });
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`另存答案新版本失败：${String(err)}`);
      }
    } finally {
      dispatchAnswerSource({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SOURCE_FINISHED" },
      });
      release();
    }
  }

  function changeRubricPointMapping(
    assessmentItemId: number,
    orderIndex: number,
    stableId: string,
  ) {
    dispatchAnswerSource({
      type: "RUBRIC_MAPPING_CHANGED",
      assessmentItemId,
      orderIndex,
      stableId,
    });
  }

  return {
    analyzeAnswerSource,
    confirmMatchingAnswerSource,
    keepCurrentBoundAnswers,
    adoptAnswerSourceAsNewVersion,
    changeRubricPointMapping,
  };
}
