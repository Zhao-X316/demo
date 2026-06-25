import { call } from "./client";

export interface SubmissionCard {
  submission_id: number;
  status: string;
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

export interface TodayView {
  date: string;
  normal: TaskCard[];
  makeup: TaskCard[];
  review: TaskCard[];
}

export const dashboardToday = () => call<TodayView>("dashboard_today");

export const humanDecide = (submission_id: number, result: string, note?: string) =>
  call<string>("verdict_human_decide", { submission_id, result, note: note ?? null });

export const seedDemo = () => call<string>("seed_demo");
