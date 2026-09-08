import { open } from "@tauri-apps/plugin-dialog";

import {
  type GroupedPageEvidence,
  examDictationAnalyzeTemplate,
  examDictationConfirmTemplate,
  examDictationProcessPage,
  examDictationTemplateStatus,
} from "../../api/exam.ts";
import type { FixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import { startFixedIntakeReadEffect } from "./fixedIntakeLifecycle.ts";
import {
  fixedIntakeCompletionIdentity,
  fixedIntakeMaterialScopeKey,
} from "./fixedIntakeRootState.ts";
import type { FixedIntakeViewModel } from "./fixedIntakeViewModel.ts";

export function createFixedIntakeDictationCommands({
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
    dictationTemplateRun,
    dictationPageResults,
    dictationPageFailures,
    dictationReferencePageId,
    dictationTemplateScopeKey,
  } = view;

  function startDictationTemplateStatusReadEffect() {
    if (!dictationTemplateScopeKey || !dictationReferencePageId) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey: dictationTemplateScopeKey,
    };
    return startFixedIntakeReadEffect({
      claim: () => claimOperation(
        `dictation-template-status:${dictationTemplateScopeKey}`,
      ),
      load: () => examDictationTemplateStatus(dictationReferencePageId),
      isCurrent: () => completionIsCurrent(identity),
      onStarted: () => dispatchBatchWorkflow({ type: "DICTATION_STATUS_LOAD_STARTED" }),
      onSucceeded: (status) => {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "DICTATION_STATUS_LOAD_SUCCEEDED", status },
        });
        if (status.activeTemplate && !runtime.resume) {
          void processDictationPages(groupingEvidence);
        }
      },
      onFailed: (err) => {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "DICTATION_STATUS_LOAD_FAILED" },
        });
        onError(`读取默写模板状态失败：${String(err)}`);
      },
    });
  }

  async function pickAndAnalyzeDictationTemplate() {
    const scopeKey = fixedIntakeMaterialScopeKey(
      stateRef.current,
      "dictation",
    );
    const referencePageId = dictationReferencePageId;
    const targetAssessmentVersionId = assessmentVersionId;
    if (!scopeKey || !referencePageId) {
      onError("当前没有可用于建立模板的已确认默写页面");
      return;
    }
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey,
      pageId: referencePageId,
    };
    const release = claimOperation(`dictation-template:${scopeKey}:analysis`);
    if (!release) return;
    let started = false;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "默写空白页", extensions: ["jpg", "jpeg", "png", "webp"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({ type: "DICTATION_TEMPLATE_ANALYSIS_STARTED" });
      started = true;
      const run = await examDictationAnalyzeTemplate(
        referencePageId,
        selected,
        `dictation-template:${targetAssessmentVersionId}:${crypto.randomUUID()}`,
      );
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED", run },
      });
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`建立默写模板失败：${String(err)}`);
      }
    } finally {
      if (started) {
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: { type: "DICTATION_TEMPLATE_OPERATION_FINISHED" },
        });
      }
      release();
    }
  }

  async function confirmDictationTemplate() {
    const scopeKey = fixedIntakeMaterialScopeKey(
      stateRef.current,
      "dictation",
    );
    const referencePageId = dictationReferencePageId;
    const run = dictationTemplateRun;
    if (!scopeKey || !referencePageId || !run?.output) return;
    const identity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey,
      pageId: referencePageId,
      dictationTemplateRunId: run.ai_run_id,
    };
    const release = claimOperation(
      `dictation-template:${scopeKey}:run:${run.ai_run_id}`,
    );
    if (!release) return;
    dispatchBatchWorkflow({ type: "DICTATION_TEMPLATE_CONFIRMATION_STARTED" });
    try {
      const confirmed = await examDictationConfirmTemplate(
        referencePageId,
        run.ai_run_id,
      );
      if (!completionIsCurrent(identity)) return;
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: {
          type: "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED",
          status: {
            assessmentVersionId: confirmed.template.assessment_version_id,
            pageNo: confirmed.template.page_no,
            activeTemplate: confirmed.template,
          },
        },
      });
      if (completionIsCurrent(identity)) {
        await processDictationPages(groupingEvidence);
      }
    } catch (err) {
      if (completionIsCurrent(identity)) {
        onError(`确认默写模板失败：${String(err)}`);
      }
    } finally {
      dispatchBatchWorkflow({
        type: "FIXED_INTAKE_COMPLETION_RECEIVED",
        identity,
        completion: { type: "DICTATION_TEMPLATE_OPERATION_FINISHED" },
      });
      release();
    }
  }

  async function processDictationPages(
    evidence: GroupedPageEvidence[] = groupingEvidence,
    retryFailed = false,
  ) {
    const pages = evidence.flatMap((group) => group.pages).filter((page) =>
      page.qualityResult === "pass"
      && page.matchDecision === "teacher_confirmed"
      && (retryFailed
        ? Boolean(dictationPageFailures[page.pageId])
        : !dictationPageResults[page.pageId] && !dictationPageFailures[page.pageId]),
    );
    if (!pages.length) return;
    const scopeKey = fixedIntakeMaterialScopeKey(
      stateRef.current,
      "dictation",
    );
    if (!scopeKey) return;
    const baseIdentity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      scopeKey,
    };
    const claimedPages = pages.flatMap((page) => {
      const release = claimOperation(`dictation-page:${page.pageId}`);
      return release
        ? [{ page, release, identity: { ...baseIdentity, pageId: page.pageId } }]
        : [];
    });
    if (!claimedPages.length) return;
    dispatchBatchWorkflow({
      type: "DICTATION_PAGES_STARTED",
      pageIds: claimedPages.map(({ page }) => page.pageId),
    });
    const failures: string[] = [];
    for (const { page, release, identity } of claimedPages) {
      if (!completionIsCurrent(identity)) {
        release();
        continue;
      }
      try {
        const processed = await examDictationProcessPage(page.pageId);
        dispatchBatchWorkflow({
          type: "FIXED_INTAKE_COMPLETION_RECEIVED",
          identity,
          completion: {
            type: "DICTATION_PAGE_SUCCEEDED",
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
              type: "DICTATION_PAGE_FAILED",
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
          completion: { type: "DICTATION_PAGE_FINISHED", pageId: page.pageId },
        });
        release();
      }
    }
    if (failures.length && completionIsCurrent(baseIdentity)) {
      onError(`有 ${failures.length} 张默写需要重拍、重试或老师检查。${failures[0]}`);
    }
  }

  return {
    dictationTemplateScopeKey,
    startDictationTemplateStatusReadEffect,
    pickAndAnalyzeDictationTemplate,
    confirmDictationTemplate,
    processDictationPages,
  };
}
