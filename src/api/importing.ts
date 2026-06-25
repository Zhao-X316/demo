import { call } from "./client";

export interface ImportResult {
  file: string;
  status: string; // imported | duplicate | anomaly | error
  detail: string;
  submission_id: number | null;
}

export interface ScoreOutcome {
  verdict_id: number;
  accuracy: number;
  pass: boolean;
  fluency: number;
  quality: string;
  text: string;
  next: string;
}

export const importPaths = (paths: string[]) => call<ImportResult[]>("import_paths", { paths });
export const asrAndScore = (submission_id: number) =>
  call<ScoreOutcome>("asr_and_score", { submission_id });
