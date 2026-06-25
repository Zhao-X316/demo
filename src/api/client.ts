import { invoke } from "@tauri-apps/api/core";

/// 统一命令调用封装。
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}
