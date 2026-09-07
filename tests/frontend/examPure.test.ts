import assert from "node:assert/strict";
import test from "node:test";

import {
  answerJsonLabel,
  displayTime,
  fileName,
  fillAnswerSlots,
  hasExtension,
  parseRequiredScore,
  shortAnswerRubricPoints,
} from "../../src/pages/exam/examPure.ts";

test("missing teacher scores are invalid rather than zero", () => {
  for (const raw of ["", " ", "\t\n", "\u3000"]) {
    assert.ok(Number.isNaN(parseRequiredScore(raw)), JSON.stringify(raw));
  }
});

test("explicit scores preserve zero, decimals and values for caller range checks", () => {
  for (const [raw, expected] of [
    ["0", 0], [" 0 ", 0], ["2.5", 2.5], [" 2.5 ", 2.5],
    ["-1", -1], ["100", 100],
  ] as const) {
    assert.equal(parseRequiredScore(raw), expected);
  }
});

test("invalid and nonfinite teacher scores remain blocked by finite checks", () => {
  for (const raw of ["not a score", "NaN", "Infinity", "-Infinity", "1e309"]) {
    assert.equal(Number.isFinite(parseRequiredScore(raw)), false, raw);
  }
});

test("hasExtension preserves extension matching behavior", () => {
  const cases = [
    { path: "PAPER.PDF", extensions: ["pdf"], expected: true },
    { path: "scan.JPG?download=1#page", extensions: ["jpg", "png"], expected: true },
    { path: "notes.txt", extensions: ["pdf"], expected: false },
    { path: "archivepdf", extensions: ["pdf"], expected: false },
  ];

  for (const example of cases) {
    assert.equal(hasExtension(example.path, example.extensions), example.expected);
  }
});

test("fileName preserves Unix, Windows, plain, and trailing separator behavior", () => {
  const cases = [
    { path: "/tmp/paper.pdf", expected: "paper.pdf" },
    { path: "C:\\work\\answer.docx", expected: "answer.docx" },
    { path: "plain.txt", expected: "plain.txt" },
    { path: "/tmp/folder/", expected: "/tmp/folder/" },
  ];

  for (const example of cases) {
    assert.equal(fileName(example.path), example.expected);
  }
});

test("answerJsonLabel preserves every supported answer summary and fallback", () => {
  const unknownJson = JSON.stringify({ unexpected: "保留原文" });
  const cases: Array<{ raw: string | null; expected: string }> = [
    { raw: null, expected: "未识别到答案" },
    { raw: "", expected: "未识别到答案" },
    { raw: JSON.stringify({ correct_labels: ["A", "C"] }), expected: "A、C" },
    { raw: JSON.stringify({ correct: true }), expected: "正确" },
    { raw: JSON.stringify({ correct: false }), expected: "错误" },
    { raw: JSON.stringify({ values: ["1840", "鸦片战争"] }), expected: "1840 / 鸦片战争" },
    { raw: JSON.stringify({ canonical_answers: ["南京", "广州"] }), expected: "南京 / 广州" },
    {
      raw: JSON.stringify({
        slots: [
          { canonical_answers: ["1840", "一八四零"] },
          { canonical_answers: ["林则徐"] },
        ],
      }),
      expected: "1840/一八四零；林则徐",
    },
    { raw: JSON.stringify({ slots: [{ canonical_answers: [] }, {}] }), expected: "填空答案待复核" },
    { raw: JSON.stringify({ reference_answer: "只学习西方技术" }), expected: "只学习西方技术" },
    { raw: unknownJson, expected: unknownJson },
    { raw: "not-json", expected: "not-json" },
  ];

  for (const example of cases) {
    assert.equal(answerJsonLabel(example.raw), example.expected);
  }
});

test("shortAnswerRubricPoints preserves filtering, fields, and conservative defaults", () => {
  for (const raw of [null, "not-json", JSON.stringify({ other: [] })]) {
    assert.deepEqual(shortAnswerRubricPoints(raw), []);
  }

  const raw = JSON.stringify({
    rubric_points: [
      null,
      {
        source_public_id: "source-1",
        stable_id: "stable-1",
        order_index: 4,
        canonical_text: "洋务运动只学习西方技术",
        max_score: 2,
      },
      "ignored",
      {},
      [],
    ],
  });

  assert.deepEqual(shortAnswerRubricPoints(raw), [
    {
      sourcePublicId: "source-1",
      stableId: "stable-1",
      orderIndex: 4,
      canonicalText: "洋务运动只学习西方技术",
      maxScore: 2,
    },
    {
      sourcePublicId: "",
      stableId: "point-3",
      orderIndex: 3,
      canonicalText: "未识别评分点",
      maxScore: 0,
    },
  ]);
});

test("fillAnswerSlots preserves filtering, canonical answers, and conservative fallbacks", () => {
  for (const raw of [null, "not-json", JSON.stringify({ other: [] })]) {
    assert.deepEqual(fillAnswerSlots(raw), []);
  }

  const raw = JSON.stringify({
    answer_slots: [
      null,
      {
        source_public_id: "source-slot-1",
        stable_id: "slot-stable-1",
        order_index: 5,
        canonical_answers_json: JSON.stringify({ answers: ["1840", 1840, "一八四零"] }),
        max_score: 2,
      },
      "ignored",
      {},
      { canonical_answers_json: "not-json" },
      [],
    ],
  });

  assert.deepEqual(fillAnswerSlots(raw), [
    {
      sourcePublicId: "source-slot-1",
      stableId: "slot-stable-1",
      orderIndex: 5,
      canonicalText: "1840 / 一八四零",
      maxScore: 2,
    },
    {
      sourcePublicId: "",
      stableId: "slot-3",
      orderIndex: 3,
      canonicalText: "标准答案待核对",
      maxScore: 0,
    },
    {
      sourcePublicId: "",
      stableId: "slot-4",
      orderIndex: 4,
      canonicalText: "标准答案待核对",
      maxScore: 0,
    },
  ]);
});

test("displayTime preserves replacement and minute truncation behavior", () => {
  const cases = [
    { value: "2026-08-01T09:19:35Z", expected: "2026-08-01 09:19" },
    { value: "08:15", expected: "08:15" },
    { value: "2026-08-01T", expected: "2026-08-01 " },
  ];

  for (const example of cases) {
    assert.equal(displayTime(example.value), example.expected);
  }
});
