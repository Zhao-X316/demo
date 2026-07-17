import { call } from "./client";

export type RecitationPointState =
  | "covered"
  | "partial"
  | "omitted"
  | "contradiction"
  | "uncertain";

export interface StructuredPointCard {
  point_result_id: number;
  rubric_point_id: number;
  stable_key: string;
  canonical_text: string;
  order_index: number;
  machine_state: RecitationPointState;
  confidence: number;
  evidence_spans: Array<{ start_ms?: number; end_ms?: number; text?: string }>;
  reason: string;
  teacher_confirmation_level: "accepted" | "corrected" | null;
  teacher_state: RecitationPointState | null;
  teacher_note: string | null;
}

export interface StructuredScoreCard {
  score_run_id: number;
  rubric_version_id: number;
  overall_suggestion: "pass" | "fail" | "unable_to_score";
  confidence: number;
  review_revision: number | null;
  points: StructuredPointCard[];
}

export interface SubmissionCard {
  submission_id: number;
  status: string;
  recognize_status: string;
  pending_review: boolean;
  file_path: string;
  recognized_text: string | null;
  answer_text: string | null;
  answer_version: number | null;
  scored_answer_version: number | null;
  accuracy: number | null;
  pass: boolean | null;
  fluency: number | null;
  quality: string | null;
  human_result: string | null;
  human_note: string | null;
  machine_note: string | null;
  structured_score: StructuredScoreCard | null;
}

export interface TaskCard {
  task_id: number;
  kind: string;
  status: string;
  due_date: string;
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
  overdue_review: TaskCard[];
}

export const dashboardToday = () => call<TodayView>("dashboard_today");

export interface Rollover {
  rolled: number; // 结转补背数
  reviews: number; // 到期复习数
}
export const dayRollover = () => call<Rollover>("day_rollover");

export interface TeacherPointReviewItemInput {
  point_result_id: number;
  confirmation_level: "accepted" | "corrected";
  corrected_state: RecitationPointState | null;
  corrected_evidence_spans_json: string | null;
  teacher_note: string | null;
}

export interface TeacherPointReviewInput {
  score_run_id: number;
  items: TeacherPointReviewItemInput[];
}

export const humanDecide = (
  submission_id: number,
  result: string,
  note?: string,
  point_review?: TeacherPointReviewInput,
) =>
  call<string>("verdict_human_decide", {
    submissionId: submission_id,
    result,
    note: note ?? null,
    pointReview: point_review ?? null,
  });

export const seedDemo = () => call<string>("seed_demo");

// 撤销一条已布置任务（仅未开始的）
export const taskCancel = (task_id: number) => call<void>("task_cancel", { taskId: task_id });
