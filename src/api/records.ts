import { call } from "./client";
import { TaskCard } from "./dashboard";

// 导入历史一行（来自后端 submissions 表，切走再回来不丢）。
export interface ImportHistoryRow {
  submission_id: number;
  file_name: string; // 当前文件名（智能识别改名后即「改后名」）
  student: string | null; // 张明 (2023001)
  content: string | null; // 道法8上-04课-05 为什么要以礼待人
  status: string; // 已评分 / 未识别·待改派 / 待分析 …
  recognized: string | null;
  has_evidence_detail: boolean;
}

export const importHistory = (limit?: number) =>
  call<ImportHistoryRow[]>("import_history", { limit: limit ?? null });

export const recitationSubmissionDetail = (submissionId: number) =>
  call<TaskCard>("recitation_submission_detail", { submissionId });
