import {
  type GroupedPageEvidence,
  examObjectiveRecognizeRegion,
  examOrdinaryPaperAnalyzePage,
  examOrdinaryPaperConfirmPageStructure,
  examOrdinaryPaperSyncQuestions,
} from "../../api/exam.ts";
import type { FixedIntakeControllerRuntime } from "./fixedIntakeControllerRuntime.ts";
import { fixedIntakeCompletionIdentity } from "./fixedIntakeRootState.ts";
import type { FixedIntakeViewModel } from "./fixedIntakeViewModel.ts";

export function createFixedIntakeOrdinaryCommands({
  runtime,
  view,
  onDone,
  onError,
}: {
  runtime: FixedIntakeControllerRuntime;
  view: FixedIntakeViewModel;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const {
    stateRef,
    dispatchBatchWorkflow,
    completionIsCurrent,
    claimOperation,
  } = runtime;
  const {
    result,
    groupingEvidence,
    ordinaryPaperRuns,
    ordinaryConfirmations,
  } = view;

  async function analyzeOrdinaryPages(
    evidence: GroupedPageEvidence[] = groupingEvidence,
    retryFailed = false,
  ) {
    if (!result) return;
    const pages = evidence.flatMap((group) => group.pages).filter((page) =>
      page.qualityResult === "pass"
      && page.matchDecision === "teacher_confirmed"
      && (retryFailed ? ordinaryPaperRuns[page.pageId]?.status === "failed" : !ordinaryPaperRuns[page.pageId]),
    );
    if (!pages.length) return;
    const batchIdentity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
    };
    const release = claimOperation(`ordinary-analyze:${result.batchId}`);
    if (!release) return;
    dispatchBatchWorkflow({
      type: "ORDINARY_ANALYSIS_STARTED",
      pageIds: pages.map((page) => page.pageId),
    });
    const failures: string[] = [];
    try {
      for (const page of pages) {
        const identity = { ...batchIdentity, pageId: page.pageId };
        if (!completionIsCurrent(identity)) break;
        const key = retryFailed
          ? `ordinary-paper:${page.pageId}:retry:${crypto.randomUUID()}`
          : `ordinary-paper:${page.pageId}:structure:v1`;
        try {
          const run = await examOrdinaryPaperAnalyzePage(page.pageId, key);
          if (!completionIsCurrent(identity)) break;
          dispatchBatchWorkflow({
            type: "FIXED_INTAKE_COMPLETION_RECEIVED",
            identity,
            completion: {
              type: "ORDINARY_ANALYSIS_SUCCEEDED",
              pageId: page.pageId,
              run,
            },
          });
        } catch (err) {
          if (!completionIsCurrent(identity)) break;
          failures.push(`第${page.pageNo}页：${String(err)}`);
        } finally {
          dispatchBatchWorkflow({
            type: "FIXED_INTAKE_COMPLETION_RECEIVED",
            identity,
            completion: { type: "ORDINARY_ANALYSIS_FINISHED", pageId: page.pageId },
          });
        }
      }
      if (failures.length && completionIsCurrent(batchIdentity)) {
        onError(`有 ${failures.length} 页未能启动普通卷分析；其余页面已继续处理。${failures[0]}`);
      }
    } finally {
      release();
    }
  }

  async function confirmReadyOrdinaryPages() {
    if (!result) return;
    const ready = Object.values(ordinaryPaperRuns).filter((run) =>
      run.status === "succeeded"
      && run.output?.state === "ready"
      && !ordinaryConfirmations[run.output.page_id],
    );
    if (!ready.length) return;
    const batchIdentity = {
      ...fixedIntakeCompletionIdentity(stateRef.current),
      batchId: result.batchId,
    };
    const release = claimOperation(`ordinary-confirm:${result.batchId}`);
    if (!release) return;
    dispatchBatchWorkflow({
      type: "ORDINARY_CONFIRMATION_STARTED",
      pageIds: ready.map((run) => run.output!.page_id),
    });
    const failures: string[] = [];
    let recognizedRegions = 0;
    let libraryMatched = 0;
    let libraryCandidates = 0;
    let libraryNeedsReview = 0;
    try {
      pageLoop: for (const run of ready) {
        const pageId = run.output!.page_id;
        const identity = {
          ...batchIdentity,
          pageId,
          ordinaryRunId: run.ai_run_id,
        };
        if (!completionIsCurrent(identity)) break;
        try {
          const confirmed = await examOrdinaryPaperConfirmPageStructure(pageId, run.ai_run_id);
          if (!completionIsCurrent(identity)) break;
          dispatchBatchWorkflow({
            type: "FIXED_INTAKE_COMPLETION_RECEIVED",
            identity,
            completion: {
              type: "ORDINARY_CONFIRMATION_SUCCEEDED",
              pageId,
              confirmation: confirmed,
            },
          });
          if (!completionIsCurrent(identity)) break;
          try {
            const synced = await examOrdinaryPaperSyncQuestions(pageId, run.ai_run_id);
            if (!completionIsCurrent(identity)) break pageLoop;
            libraryMatched += synced.matched_count;
            libraryCandidates += synced.candidate_created_count;
            libraryNeedsReview += synced.needs_review_count
              + synced.privacy_rejected_count
              + synced.low_confidence_skipped_count
              + synced.failed_count;
          } catch (err) {
            if (!completionIsCurrent(identity)) break pageLoop;
            // 题库沉淀是可恢复旁路，绝不能阻断当前学生作业继续识别和批改。
            failures.push(`第${run.output!.expected_page_no}页题库沉淀：${String(err)}`);
          }
          for (const region of confirmed.regions) {
            if (!completionIsCurrent(identity)) break pageLoop;
            try {
              await examObjectiveRecognizeRegion(
                region.id,
                `ordinary-objective:${region.id}:v1`,
              );
              if (!completionIsCurrent(identity)) break pageLoop;
              recognizedRegions += 1;
            } catch (err) {
              if (!completionIsCurrent(identity)) break pageLoop;
              failures.push(`第${run.output!.expected_page_no}页第${region.region_index + 1}区：${String(err)}`);
            }
          }
        } catch (err) {
          if (!completionIsCurrent(identity)) break;
          failures.push(`第${run.output!.expected_page_no}页：${String(err)}`);
        } finally {
          dispatchBatchWorkflow({
            type: "FIXED_INTAKE_COMPLETION_RECEIVED",
            identity,
            completion: { type: "ORDINARY_CONFIRMATION_FINISHED", pageId },
          });
        }
      }
      if (!completionIsCurrent(batchIdentity)) return;
      if (failures.length) {
        onError(`已完成 ${recognizedRegions} 个题区识别，另有 ${failures.length} 项需重试。${failures[0]}`);
      } else if (libraryMatched + libraryCandidates + libraryNeedsReview > 0) {
        onDone(
          `普通卷已进入批改：识别 ${recognizedRegions} 个题区；题库复用 ${libraryMatched} 题，新增私有候选 ${libraryCandidates} 题，待整理 ${libraryNeedsReview} 题。`,
        );
      }
    } finally {
      release();
    }
  }

  return {
    analyzeOrdinaryPages,
    confirmReadyOrdinaryPages,
  };
}
