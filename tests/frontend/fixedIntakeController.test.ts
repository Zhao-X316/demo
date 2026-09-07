import assert from "node:assert/strict";
import test from "node:test";

import type { FixedIntakeOption } from "../../src/api/exam.ts";
import { createInitialFixedIntakeState } from "../../src/pages/exam/fixedIntakeRootState.ts";
import {
  createFixedIntakeViewModel,
  fixedIntakeAssessmentOptions,
  fixedIntakeClassOptions,
} from "../../src/pages/exam/fixedIntakeViewModel.ts";

const options: FixedIntakeOption[] = [
  {
    classId: 2,
    className: "八年级二班",
    assessmentId: 20,
    assessmentVersionId: 201,
    assessmentTitle: "第一次默写",
    revision: 1,
    templateVersion: null,
    itemCount: 3,
    isDefault: false,
    defaultSelectionPublicId: null,
  },
  {
    classId: 1,
    className: "八年级一班",
    assessmentId: 10,
    assessmentVersionId: 101,
    assessmentTitle: "单元测验",
    revision: 1,
    templateVersion: "v1",
    itemCount: 5,
    isDefault: true,
    defaultSelectionPublicId: "selection-1",
  },
  {
    classId: 2,
    className: "八年级二班（新）",
    assessmentId: 21,
    assessmentVersionId: 202,
    assessmentTitle: "第二次默写",
    revision: 1,
    templateVersion: null,
    itemCount: 2,
    isDefault: false,
    defaultSelectionPublicId: null,
  },
];

test("fixed intake options preserve class insertion order and latest duplicate name", () => {
  assert.deepEqual(fixedIntakeClassOptions(options), [
    { id: 2, name: "八年级二班（新）" },
    { id: 1, name: "八年级一班" },
  ]);
  assert.deepEqual(
    fixedIntakeAssessmentOptions(options, 2).map((option) => option.assessmentVersionId),
    [201, 202],
  );
});

test("fixed intake initial view model is pure and keeps empty progress semantics", () => {
  const state = createInitialFixedIntakeState({ classId: 2, assessmentVersionId: 201 });
  const snapshot = structuredClone(state);
  const view = createFixedIntakeViewModel(options, state);

  assert.deepEqual(state, snapshot);
  assert.equal(view.routeLabel, "暂时受阻");
  assert.deepEqual(view.groupingAbsenceCandidates, []);
  assert.deepEqual(view.answerSheetEligiblePages, []);
  assert.equal(view.answerSheetTemplateScopeKey, "");
  assert.equal(view.dictationTemplateScopeKey, "");
  assert.equal(view.ordinaryEligiblePageCount, 0);
  assert.equal(view.ordinaryPendingCount, 0);
  assert.equal(view.answerSheetPendingCount, 0);
  assert.equal(view.dictationPendingCount, 0);
});
