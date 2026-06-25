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
    submission_id,
    student_no: student_no ?? null,
    content_no: content_no ?? null,
  });
