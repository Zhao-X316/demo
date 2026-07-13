import fixture from "../../contracts/app_error.v1.json";

export type AppErrorDto = {
  code: string;
  message: string;
  retryable: boolean;
  actionHint: string | null;
  details: Record<string, string>;
};

// Rust 与 TypeScript 共读固定 fixture；生产 command 将按模块逐步迁移到此结构。
export const APP_ERROR_CONTRACT_FIXTURE = fixture satisfies AppErrorDto;

export function isAppErrorDto(value: unknown): value is AppErrorDto {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<AppErrorDto>;
  return (
    typeof candidate.code === "string" &&
    typeof candidate.message === "string" &&
    typeof candidate.retryable === "boolean" &&
    (typeof candidate.actionHint === "string" || candidate.actionHint === null) &&
    !!candidate.details &&
    typeof candidate.details === "object"
  );
}
