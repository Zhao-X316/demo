import { K1QuestionType, SourceInboxItem } from "../../api/knowledge";

export const TYPE_LABEL: Record<K1QuestionType, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
  fill_blank: "填空",
  short_answer: "简答",
};

export const TYPE_ORDER: K1QuestionType[] = [
  "single",
  "multiple",
  "true_false",
  "fill_blank",
  "short_answer",
];

export function sameNumber(left: number, right: number) {
  return Math.abs(left - right) < 0.000001;
}

export function sourceFileLabel(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function sourceTypeLabel(source: SourceInboxItem["document"]["sourceType"]) {
  return source === "blank_paper" ? "空白试卷" : "电子题目文件";
}

export function sourceFailureMessage(item: SourceInboxItem) {
  if (!item.latestAiErrorMetaJson) return "识别失败，来源已保留，可重新上传重试";
  try {
    const value = JSON.parse(item.latestAiErrorMetaJson) as {
      safeMessage?: unknown;
      safe_message?: unknown;
    };
    const safeMessage = value.safeMessage ?? value.safe_message;
    return typeof safeMessage === "string"
      ? safeMessage
      : "识别失败，来源已保留，可重新上传重试";
  } catch {
    return "识别失败，来源已保留，可重新上传重试";
  }
}
