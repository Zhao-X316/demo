import { call } from "./client";

export interface SubmissionCard {
  submission_id: number;
  status: string;
  file_path: string;
  recognized_text: string | null;
  accuracy: number | null;
  pass: boolean | null;
  fluency: number | null;
  quality: string | null;
  human_result: string | null;
  machine_note: string | null;
}

export interface TaskCard {
  task_id: number;
  kind: string;
  status: string;
  student_no: string;
  student_name: string;
  content_no: string;
  content_title: string;
  submission: SubmissionCard | null;
}

export interface TodayContentStat {
  content_no: string;
  content_title: string;
  should: number;
  submitted: number;
  passed: number;
  failed: number;
}

export interface TodaySummary {
  should: number;
  submitted: number;
  passed: number;
  failed: number;
  pending: number;
  makeup: number;
  contents: TodayContentStat[];
}

export interface TodayView {
  date: string;
  summary: TodaySummary;
  normal: TaskCard[];
  makeup: TaskCard[];
  review: TaskCard[];
}

export const dashboardToday = () => call<TodayView>("dashboard_today");

export interface Rollover {
  rolled: number; // 结转补背数
  reviews: number; // 到期复习数
}
export const dayRollover = () => call<Rollover>("day_rollover");

export const humanDecide = (submission_id: number, result: string, note?: string) =>
  call<string>("verdict_human_decide", { submissionId: submission_id, result, note: note ?? null });

export const seedDemo = () => call<string>("seed_demo");

// 撤销一条已布置任务（仅未开始的）
export const taskCancel = (task_id: number) => call<void>("task_cancel", { taskId: task_id });
