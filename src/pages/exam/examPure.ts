// Keep missing scores invalid until the caller's finite/range checks run.
export function parseRequiredScore(raw: string): number {
  return raw.trim() === "" ? Number.NaN : Number(raw);
}

export function hasExtension(path: string, extensions: string[]) {
  const clean = path.split(/[?#]/, 1)[0].toLowerCase();
  return extensions.some((extension) => clean.endsWith(`.${extension}`));
}

export function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

export function answerJsonLabel(raw: string | null) {
  if (!raw) return "未识别到答案";
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (Array.isArray(value.correct_labels)) return value.correct_labels.join("、");
    if (typeof value.correct === "boolean") return value.correct ? "正确" : "错误";
    if (Array.isArray(value.values)) return value.values.join(" / ");
    if (Array.isArray(value.canonical_answers)) return value.canonical_answers.join(" / ");
    if (Array.isArray(value.slots)) {
      return value.slots.map((slot) => {
        const item = slot as Record<string, unknown>;
        const answers = Array.isArray(item.canonical_answers) ? item.canonical_answers : [];
        return answers.join("/");
      }).filter(Boolean).join("；") || "填空答案待复核";
    }
    if (typeof value.reference_answer === "string") return value.reference_answer;
    return raw;
  } catch {
    return raw;
  }
}

export function shortAnswerRubricPoints(raw: string | null) {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!Array.isArray(value.rubric_points)) return [];
    return value.rubric_points.flatMap((rawPoint, index) => {
      if (!rawPoint || typeof rawPoint !== "object" || Array.isArray(rawPoint)) return [];
      const point = rawPoint as Record<string, unknown>;
      return [{
        sourcePublicId: typeof point.source_public_id === "string" ? point.source_public_id : "",
        stableId: typeof point.stable_id === "string" ? point.stable_id : `point-${index}`,
        orderIndex: typeof point.order_index === "number" ? point.order_index : index,
        canonicalText: typeof point.canonical_text === "string" ? point.canonical_text : "未识别评分点",
        maxScore: typeof point.max_score === "number" ? point.max_score : 0,
      }];
    });
  } catch {
    return [];
  }
}

export function fillAnswerSlots(raw: string | null) {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!Array.isArray(value.answer_slots)) return [];
    return value.answer_slots.flatMap((rawSlot, index) => {
      if (!rawSlot || typeof rawSlot !== "object" || Array.isArray(rawSlot)) return [];
      const slot = rawSlot as Record<string, unknown>;
      let canonicalAnswers: string[] = [];
      if (typeof slot.canonical_answers_json === "string") {
        try {
          const canonical = JSON.parse(slot.canonical_answers_json) as Record<string, unknown>;
          if (Array.isArray(canonical.answers)) {
            canonicalAnswers = canonical.answers.filter((item): item is string => typeof item === "string");
          }
        } catch {
          canonicalAnswers = [];
        }
      }
      return [{
        sourcePublicId: typeof slot.source_public_id === "string" ? slot.source_public_id : "",
        stableId: typeof slot.stable_id === "string" ? slot.stable_id : `slot-${index}`,
        orderIndex: typeof slot.order_index === "number" ? slot.order_index : index,
        canonicalText: canonicalAnswers.join(" / ") || "标准答案待核对",
        maxScore: typeof slot.max_score === "number" ? slot.max_score : 0,
      }];
    });
  } catch {
    return [];
  }
}

export function displayTime(value: string) {
  return value.replace("T", " ").slice(0, 16);
}
