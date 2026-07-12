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

export interface RecognitionFailure {
  submission_id: number;
  file_path: string;
  error_code: string;
  error_message: string;
  retryable: boolean;
  failed_at: string;
  attempts: number;
  has_task: boolean;
  file_missing: boolean;
}

export const recognitionFailuresList = () =>
  call<RecognitionFailure[]>("recognition_failures_list");

export const recognitionRelocate = (submission_id: number, new_path: string) =>
  call<void>("recognition_relocate", { submissionId: submission_id, newPath: new_path });

export const recognitionVoid = (submission_id: number) =>
  call<boolean>("recognition_void", { submissionId: submission_id });

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
