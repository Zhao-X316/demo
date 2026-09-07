

export const PROFILE_STATUS: Record<string, string> = {
  included: "已纳入",
  missing_snapshot: "无快照",
  scope_mismatch: "范围不符",
  stale_snapshot: "需更新",
  unassessed: "未评估",
  insufficient_evidence: "证据不足",
  evidence_insufficient: "证据不足",
  data_unavailable: "数据不足",
  needs_support: "需要支持",
  developing: "发展中",
  stable: "较稳定",
};

export function newRequestKey(prefix: string) {
  const random = globalThis.crypto?.randomUUID?.()
    ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return `${prefix}-${random}`;
}

export function formatDelta(value: number | null) {
  if (value === null) return "—";
  if (value > 0) return `+${value}`;
  return String(value);
}

export function profileCellClass(status: string) {
  return `class-profile-cell ${status.replace(/_/g, "-")}`;
}

export function tagClass(status: string) {
  if (status === "completed" || status === "published") return "tag pass";
  if (status === "recognition_failed") return "tag fail";
  if (["pending_review", "ready_to_publish", "grading", "ingesting", "partial"].includes(status)) {
    return "tag wait";
  }
  return "tag";
}
