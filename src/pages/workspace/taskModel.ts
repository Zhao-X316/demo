import type { WorkspaceTask } from "../../api/workspace.ts";

export type TaskFilter = "todo" | "processing" | "done";
export const taskKey = (task: Pick<WorkspaceTask, "kind" | "sourceId">) =>
  `${task.kind}:${task.sourceId}`;
export function taskFilter(task: WorkspaceTask): TaskFilter {
  if (["published", "confirmed"].includes(task.status)) return "done";
  if (task.status === "processing") return "processing";
  return "todo";
}
export function taskLabel(task: WorkspaceTask) {
  const labels: Record<string, string> = {
    published: "已发布",
    confirmed: "老师已确认",
    processing: "处理尚未结束",
    recognition_failed: "录音识别失败，需要处理",
    anomaly: "需要确认学生或内容",
    ready_to_publish: "核对完成，待发布",
    grading: "等待老师核对",
    scored: "等待老师核对",
    ingesting: "材料尚未处理完成",
    needs_material: "继续整理材料",
    incomplete: "材料登记中断，需要检查",
    pending: "录音等待处理",
    needs_review: "有内容需要老师判断",
  };
  return labels[task.status] ?? "需要继续处理";
}
export function taskAction(task: WorkspaceTask) {
  if (taskFilter(task) === "done") return "查看结果";
  if (task.status === "ready_to_publish") return "查看待发布结果";
  if (["recognition_failed", "anomaly", "pending"].includes(task.status))
    return "处理录音";
  return task.hasEvidence ? "继续核对" : "继续处理";
}

/** Scope by both version and attempt. An empty batch must never expose all attempts. */
export function scopeWorkbench<
  T extends { assessment_version_id: number; attempt_id: number },
>(rows: T[], task: WorkspaceTask | null): T[] {
  if (!task) return rows;
  const ids = new Set(task.attemptIds);
  return rows.filter(
    (row) =>
      row.assessment_version_id === task.assessmentVersionId &&
      ids.has(row.attempt_id),
  );
}

export function parseTaskLocator(
  raw: string | null,
): Pick<WorkspaceTask, "kind" | "sourceId"> | null {
  if (!raw) return null;
  const match = /^(exam_batch|exam_attempt|recitation):([1-9]\d*)$/.exec(raw);
  if (!match || !Number.isSafeInteger(Number(match[2]))) return null;
  return {
    kind: match[1] as WorkspaceTask["kind"],
    sourceId: Number(match[2]),
  };
}
