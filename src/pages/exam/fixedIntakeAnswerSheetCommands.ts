import { open } from "@tauri-apps/plugin-dialog";

import {
  type GroupedPageEvidence,
  examAnswerSheetAnalyzeTemplate,
  examAnswerSheetConfirmTemplate,
  examAnswerSheetProcessPage,
  examAnswerSheetTemplateStatus,
} from "../../api/exam.ts";
import type { FixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import { startFixedIntakeReadEffect } from "./fixedIntakeLifecycle.ts";
import {
  fixedIntakeCompletionIdentity,
  fixedIntakeMaterialScopeKey,
} from "./fixedIntakeRootState.ts";
import type { FixedIntakeViewModel } from "./fixedIntakeViewModel.ts";

export function createFixedIntakeAnswerSheetCommands({
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
    dispatchBatchWorkflow,
    completionIsCurrent,
    claimOperation,
  } = runtime;
  const {
    assessmentVersionId,
    groupingEvidence,
    answerSheetTemplateRun,
    answerSheetPageResults,
    answerSheetPageFailures,
    answerSheetReferencePageId,
    answerSheetTemplateTargetPageNo,
    answerSheetTemplateTargetPageId,
    answerSheetTemplateScopeKey,
  } = view;

  function startAnswerSheetTemplateStatusReadEffect() {
    if (!answerSheetTemplateScopeKey || !answerSheetReferencePageId) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey: answerSheetTemplateScopeKey,
    };
    return startFixedIntakeReadEffect({
      claim: () => claimOperation(
        `answer-sheet-template-status:${answerSheetTemplateScopeKey}`,
      ),
      load: () => examAnswerSheetTemplateStatus(answerSheetReferencePageId),
      isCurrent: () => completionIsCurrent(identity),
      onStarted: () => dispatchBatchWorkflow({ type: "ANSWER_SHEET_STATUS_LOAD_STARTED" }),
      onSucceeded: (status) => {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED", status },
        });
        if (status.templateSet.ready && !runtime.resume) {
          void processAnswerSheetPages(groupingEvidence);
        }
      },
      onFailed: (err) => {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SHEET_STATUS_LOAD_FAILED" },
        });
        onError(`读取答题卡模板状态失败：${String(err)}`);
      },
    });
  }

  async function pickAndAnalyzeAnswerSheetTemplate() {
    const scopeKey = fixedIntakeMaterialScopeKey(
      stateRef.current,
      "answer_sheet",
    );
    const targetPageId = answerSheetTemplateTargetPageId;
    const targetPageNo = answerSheetTemplateTargetPageNo;
    const targetAssessmentVersionId = assessmentVersionId;
    if (!scopeKey || !targetPageId) {
      onError("当前没有可用于建立模板的已确认答题卡页面");
      return;
    }
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey,
      pageId: targetPageId,
    };
    const release = claimOperation(`answer-sheet-template:${scopeKey}:analysis`);
    if (!release) return;
    let started = false;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "空白答题卡", extensions: ["jpg", "jpeg", "png", "webp"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({ type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_STARTED" });
      started = true;
      const run = await examAnswerSheetAnalyzeTemplate(
        targetPageId,
        selected,
        `answer-sheet-template:${targetAssessmentVersionId}:page:${targetPageNo}:${crypto.randomUUID()}`,
      );
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED", run },
      });
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`建立答题卡模板失败：${String(err)}`);
      }
    } finally {
      if (started) {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED" },
        });
      }
      release();
    }
  }

  async function confirmAnswerSheetTemplate() {
    const scopeKey = fixedIntakeMaterialScopeKey(
      stateRef.current,
      "answer_sheet",
    );
    const targetPageId = answerSheetTemplateTargetPageId;
    const referencePageId = answerSheetReferencePageId;
    const run = answerSheetTemplateRun;
    if (!scopeKey || !targetPageId || !referencePageId || !run?.output) return;
    const baseIdentity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey,
      pageId: targetPageId,
    };
    const runIdentity = {
      ...baseIdentity,
      answerSheetTemplateRunId: run.ai_run_id,
    };
    const release = claimOperation(
      `answer-sheet-template:${scopeKey}:run:${run.ai_run_id}`,
    );
    if (!release) return;
    dispatchBatchWorkflow({ type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_STARTED" });
    try {
      await examAnswerSheetConfirmTemplate(
        targetPageId,
        run.ai_run_id,
      );
      if (!completionIsCurrent(runIdentity)) return;
      const refreshed = await examAnswerSheetTemplateStatus(referencePageId);
      if (!completionIsCurrent(runIdentity)) return;
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity: runIdentity,
        completion: {
          type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED",
          status: refreshed,
        },
      });
      if (refreshed.templateSet.ready && completionIsCurrent(baseIdentity)) {
        await processAnswerSheetPages(groupingEvidence);
      }
    } catch (err) {
      if (completionIsCurrent(runIdentity)) {
        onError(`确认答题卡模板失败：${String(err)}`);
      }
    } finally {
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity: baseIdentity,
        completion: { type: "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED" },
      });
      release();
    }
  }

  async function processAnswerSheetPages(
    evidence: GroupedPageEvidence[] = groupingEvidence,
    retryFailed = false,
  ) {
    const pages = evidence.flatMap((group) => group.pages).filter((page) =>
      page.qualityResult === "pass"
      && page.matchDecision === "teacher_confirmed"
      && (retryFailed
        ? Boolean(answerSheetPageFailures[page.pageId])
        : !answerSheetPageResults[page.pageId] && !answerSheetPageFailures[page.pageId]),
    );
    if (!pages.length) return;
    const scopeKey = fixedIntakeMaterialScopeKey(
      stateRef.current,
      "answer_sheet",
    );
    if (!scopeKey) return;
    const baseIdentity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey,
    };
    const claimedPages = pages.flatMap((page) => {
      const release = claimOperation(`answer-sheet-page:${page.pageId}`);
      return release
        ? [{ page, release, identity: { ...baseIdentity, pageId: page.pageId } }]
        : [];
    });
    if (!claimedPages.length) return;
    dispatchBatchWorkflow({
      type: "ANSWER_SHEET_PAGES_STARTED",
      pageIds: claimedPages.map(({ page }) => page.pageId),
    });
    const failures: string[] = [];
    for (const { page, release, identity } of claimedPages) {
      if (!completionIsCurrent(identity)) {
        release();
        continue;
      }
      try {
        const processed = await examAnswerSheetProcessPage(page.pageId);
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: {
            type: "ANSWER_SHEET_PAGE_SUCCEEDED",
            pageId: page.pageId,
            result: processed,
          },
        });
      } catch (err) {
        const message = String(err);
        if (completionIsCurrent(identity)) {
          dispatchBatchWorkflow({
            type: "FIXED_INTAKE_COMPLETION_RECEIVED",
            identity,
            completion: {
              type: "ANSWER_SHEET_PAGE_FAILED",
              pageId: page.pageId,
              error: message,
            },
          });
          failures.push(`第${page.pageNo}页：${message}`);
        }
      } finally {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "ANSWER_SHEET_PAGE_FINISHED", pageId: page.pageId },
        });
        release();
      }
    }
    if (failures.length && completionIsCurrent(baseIdentity)) {
      onError(`有 ${failures.length} 张答题卡需要重试或老师检查。${failures[0]}`);
    }
  }

  return {
    answerSheetTemplateScopeKey,
    startAnswerSheetTemplateStatusReadEffect,
    pickAndAnalyzeAnswerSheetTemplate,
    confirmAnswerSheetTemplate,
    processAnswerSheetPages,
  };
}
