import { call } from "./client";

export interface Anomaly {
  submission_id: number;
  file_path: string;
  anomaly_type: string;
  parsed_meta: string | null;
  student_id: number | null;
  ref_id: number | null;
}

export const anomaliesList = () => call<Anomaly[]>("anomalies_list");

export const anomalyReassign = (submission_id: number, student_no?: string, content_no?: string) =>
  call<number | null>("anomaly_reassign", {
    submissionId: submission_id,
    studentNo: student_no ?? null,
    contentNo: content_no ?? null,
  });

export interface ContentCandidate {
  content_no: string;
  title: string;
  score: number; // 匹配度 0-100
}
export interface Suggest {
  student_no: string | null;
  student_name: string | null;
  contents: ContentCandidate[];
}
export const suggestMatch = (text: string) => call<Suggest>("suggest_match", { text });
