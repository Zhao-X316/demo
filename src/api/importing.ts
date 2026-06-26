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
  call<ScoreOutcome>("asr_and_score", { submissionId: submission_id });

export interface AutonameResult {
  file: string;
  status: string; // scored | duplicate | unmatched | error
  detail: string;
  new_name: string | null;
  student: string | null;
  content: string | null;
  accuracy: number | null;
  pass: boolean | null;
}

export const importAutoname = (paths: string[]) =>
  call<AutonameResult[]>("import_autoname", { paths });

export interface StageResult {
  file: string;
  status: string; // staged | duplicate | error
  detail: string;
}

// 第一步「导入」：只哈希去重、入待分析队列，不调 ASR
export const importStage = (paths: string[]) =>
  call<StageResult[]>("import_stage", { paths });
