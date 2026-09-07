import { WrongbookStatus, WrongbookStatistics } from "../../api/learning";

export const STATUS_LABELS: Record<WrongbookStatus, string> = {
  needs_correction: "待订正",
  corrected_once: "已订正一次",
  rechecked_correct: "再次作答正确",
};

export const QUESTION_TYPE_LABELS: Record<string, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
  fill_blank: "填空",
  short_answer: "简答",
};

export const CONTEXT_LABELS: Record<string, string> = {
  classwork: "课堂练习",
  in_class: "课堂练习",
  closed_book: "闭卷",
  homework: "家庭作业",
  quiz: "随堂测验",
  exam: "正式考试",
  open_book: "开卷",
  correction: "订正",
  demo: "演示",
};

export function statusClass(status: WrongbookStatus) {
  if (status === "needs_correction") return "tag fail";
  if (status === "corrected_once") return "tag wait";
  return "tag pass";
}

export function score(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

export function formatTime(value: string) {
  return new Date(value).toLocaleString();
}

export function shanghaiDate(value = new Date()) {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(value);
}

export function shiftShanghaiDate(date: string, days: number) {
  const value = new Date(`${date}T00:00:00+08:00`);
  value.setUTCDate(value.getUTCDate() + days);
  return shanghaiDate(value);
}

export function newRequestKey(prefix: string) {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function CauseDistributionList({
  title,
  items,
  empty,
}: {
  title: string;
  items: WrongbookStatistics["question_causes"];
  empty: string;
}) {
  return (
    <div className="wrongbook-distribution">
      <div className="wrongbook-distribution-title">
        <b>{title}</b>
        <span>仅使用老师确认错因</span>
      </div>
      {items.length === 0 ? (
        <div className="wrongbook-distribution-empty">{empty}</div>
      ) : (
        <div className="wrongbook-distribution-list">
          {items.slice(0, 8).map((item) => (
            <div key={item.public_id}>
              <div>
                <b title={item.title}>{item.title}</b>
                <span>{item.confirmed_review_count} 条已确认记录</span>
              </div>
              <div>
                {item.causes.map((cause) => (
                  <span key={cause.cause_code}>{cause.cause_label} {cause.count}</span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
