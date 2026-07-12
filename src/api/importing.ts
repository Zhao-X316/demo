import { call } from "./client";

export interface ScoreOutcome {
  verdict_id: number;
  accuracy: number;
  pass: boolean;
  fluency: number;
  quality: string;
  text: string;
  next: string;
}

export const asrAndScore = (submission_id: number) =>
  call<ScoreOutcome>("asr_and_score", { submissionId: submission_id });

export interface AutonameResult {
  file: string;
  status: string; // scored | rescored | duplicate | unmatched | error
  detail: string;
  new_name: string | null;
  student: string | null;
  content: string | null;
  accuracy: number | null;
  pass: boolean | null;
}

export const importAutoname = (paths: string[], force = false) =>
  call<AutonameResult[]>("import_autoname", { paths, force });

export interface StageResult {
  file: string;
  status: string; // staged | analyzing | done | duplicate | error
  detail: string;
}

// 第一步「导入」：只哈希去重、入待分析队列，不调 ASR。force=忽略重复强制导入
export const importStage = (paths: string[], force = false) =>
  call<StageResult[]>("import_stage", { paths, force });
