import { open } from "@tauri-apps/plugin-dialog";

import {
  type GroupedPageEvidence,
  examFixedIntakeConfirmGrouping,
  examFixedIntakeConfirmGroupingQuality,
  examFixedIntakeConfirmMaterialType,
  examFixedIntakeGroupingEvidence,
  examFixedIntakeReplaceRejectedPage,
} from "../../api/exam.ts";
import type { FixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import { startFixedIntakeReadEffect } from "./fixedIntakeLifecycle.ts";
import { fixedIntakeCompletionIdentity } from "./fixedIntakeRootState.ts";
import type { FixedIntakeViewModel } from "./fixedIntakeViewModel.ts";

export function createFixedIntakeGroupingQualityCommands({
  runtime,
  view,
  analyzeOrdinaryPages,
  onError,
}: {
  runtime: FixedIntakeControllerRuntime;
  view: FixedIntakeViewModel;
  analyzeOrdinaryPages: (
    evidence?: GroupedPageEvidence[],
    retryFailed?: boolean,
  ) => Promise<void>;
  onError: (message: string) => void;
}) {
  const {
    stateRef,
    dispatchBatchWorkflow,
    dispatchBatchWorkflowBeforeContinuation,
    completionIsCurrent,
    claimOperation,
  } = runtime;
  const {
    result,
    groupingStartNo,
    absentStudentNos,
    rejectedPageIds,
  } = view;

  const confirmGrouping = async () => {
    if (!result || !groupingStartNo) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
    };
    const release = claimOperation(`grouping:${result.batchId}`);
    if (!release) return;
    dispatchBatchWorkflow({ type: "GROUPING_CONFIRMATION_STARTED" });
    try {
      const confirmed = await examFixedIntakeConfirmGrouping(
        result.batchId,
        groupingStartNo,
        absentStudentNos,
      );
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "GROUPING_CONFIRMATION_SUCCEEDED", confirmation: confirmed },
      });
      if (completionIsCurrent(identity)) {
        dispatchBatchWorkflow({ type: "ORDINARY_FULL_RESET" });
        dispatchBatchWorkflow({ type: "ANSWER_SHEET_RESET" });
        dispatchBatchWorkflow({ type: "DICTATION_RESET" });
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(String(err));
      }
    } finally {
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "GROUPING_CONFIRMATION_FINISHED" },
      });
      release();
    }
  };

  const groupingEvidenceBatchId = result?.groupingConfirmed ? result.batchId : null;

  function startGroupingEvidenceReadEffect() {
    if (!groupingEvidenceBatchId) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: groupingEvidenceBatchId,
    };
    return startFixedIntakeReadEffect({
      claim: () => claimOperation(`grouping-evidence:${groupingEvidenceBatchId}`),
      load: () => examFixedIntakeGroupingEvidence(groupingEvidenceBatchId),
      isCurrent: () => completionIsCurrent(identity),
      onStarted: () => dispatchBatchWorkflow({ type: "GROUPING_EVIDENCE_STARTED" }),
      onSucceeded: (evidence) => dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "GROUPING_EVIDENCE_SUCCEEDED", evidence },
      }),
      onFailed: (err) => onError(`读取归组照片失败：${String(err)}`),
      onFinished: () => dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "GROUPING_EVIDENCE_FINISHED" },
      }),
    });
  }

  const toggleRejectedPage = (pageId: number) => {
    dispatchBatchWorkflow({ type: "QUALITY_REJECTION_TOGGLED", pageId });
  };

  const confirmGroupingQuality = async () => {
    if (!result) return;
    const batchId = result.batchId;
    const materialType = result.materialType;
    const rejectedPageIdSnapshot = [...rejectedPageIds];
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId,
    };
    const release = claimOperation(`quality:${batchId}`);
    if (!release) return;
    dispatchBatchWorkflow({ type: "QUALITY_CONFIRMATION_STARTED" });
    try {
      const confirmed = await examFixedIntakeConfirmGroupingQuality(
        batchId,
        rejectedPageIdSnapshot,
      );
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "QUALITY_CONFIRMATION_SUCCEEDED", confirmation: confirmed },
      });
      const evidence = await examFixedIntakeGroupingEvidence(batchId);
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflowBeforeContinuation({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "GROUPING_EVIDENCE_SUCCEEDED", evidence },
      });
      if (materialType === "ordinary_paper") {
        void analyzeOrdinaryPages(evidence);
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(String(err));
      }
    } finally {
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "QUALITY_CONFIRMATION_FINISHED" },
      });
      release();
    }
  };

  const replaceRejectedPage = async (pageId: number) => {
    if (!result) return;
    const batchId = result.batchId;
    const materialType = result.materialType;
    const continuationIdentity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId,
    };
    const identity = {
      ...continuationIdentity,
      rejectedPageId: pageId,
    };
    const release = claimOperation(`retake:${batchId}`);
    if (!release) return;
    let started = false;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "重拍照片", extensions: ["jpg", "jpeg"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({ type: "RETAKE_STARTED", pageId });
      started = true;
      const replaced = await examFixedIntakeReplaceRejectedPage(batchId, pageId, selected);
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "RETAKE_SUCCEEDED", replacement: replaced },
      });
      const evidence = await examFixedIntakeGroupingEvidence(batchId);
      if (!completionIsCurrent(continuationIdentity)) return;
      dispatchBatchWorkflowBeforeContinuation({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity: continuationIdentity,
        completion: { type: "GROUPING_EVIDENCE_SUCCEEDED", evidence },
      });
      if (materialType === "ordinary_paper" && replaced.activatedStudent) {
        void analyzeOrdinaryPages(evidence);
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`替换重拍页失败：${String(err)}`);
      }
    } finally {
      if (started) {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity: continuationIdentity,
          completion: { type: "RETAKE_FINISHED" },
        });
      }
      release();
    }
  };

  const toggleAbsentStudent = (studentNo: string) => {
    dispatchBatchWorkflow({ type: "ABSENT_STUDENT_TOGGLED", studentNo });
  };

  const changeGroupingStart = (studentNo: string) => {
    dispatchBatchWorkflow({ type: "GROUPING_START_CHANGED", studentNo });
  };

  const confirmMaterialType = async (
    materialType: "ordinary_paper" | "answer_sheet" | "dictation",
  ) => {
    if (!result) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
    };
    const release = claimOperation(`material:${result.batchId}`);
    if (!release) return;
    dispatchBatchWorkflow({ type: "MATERIAL_CONFIRMATION_STARTED" });
    try {
      const confirmed = await examFixedIntakeConfirmMaterialType(
        result.batchId,
        materialType,
      );
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "MATERIAL_CONFIRMATION_SUCCEEDED", confirmation: confirmed },
      });
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(String(err));
      }
    } finally {
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "MATERIAL_CONFIRMATION_FINISHED" },
      });
      release();
    }
  };

  return {
    groupingEvidenceBatchId,
    startGroupingEvidenceReadEffect,
    confirmGrouping,
    toggleRejectedPage,
    confirmGroupingQuality,
    replaceRejectedPage,
    toggleAbsentStudent,
    changeGroupingStart,
    confirmMaterialType,
  };
}
