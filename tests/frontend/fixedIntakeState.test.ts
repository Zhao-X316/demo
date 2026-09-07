import assert from "node:assert/strict";
import test from "node:test";

import type {
  AnswerSheetPageProcessingResult,
  AnswerSheetTemplateRunResult,
  AnswerSheetTemplateStatus,
  AnswerSourceAnalysisResult,
  AnswerSourceReviewItem,
  AnswerSourceReviewSummary,
  DictationPageProcessingResult,
  DictationTemplateRunResult,
  DictationTemplateStatus,
  FixedIntakeResult,
  GroupedPageEvidence,
  OrdinaryPaperRunResult,
  OrdinaryStructureConfirmationResult,
  PageCycleSuggestion,
} from "../../src/api/exam.ts";
import {
  NEW_RUBRIC_POINT,
  createInitialAnswerSourceState,
  createInitialFixedIntakeBatchWorkflowState,
  fixedIntakeAnswerSourceReducer,
  fixedIntakeBatchWorkflowReducer,
  initialRubricPointMappings,
  rubricPointMappingKey,
  rubricPointMappingsReady,
} from "../../src/pages/exam/fixedIntakeState.ts";
import {
  createInitialFixedIntakeState,
  fixedIntakeReducer,
} from "../../src/pages/exam/fixedIntakeRootState.ts";

function boundPoint(stableId: string, orderIndex: number) {
  return {
    stableId,
    orderIndex,
    canonicalText: `旧评分点 ${orderIndex + 1}`,
    maxScore: 2,
    confirmedKnowledgeTitles: [],
    confirmedAbilityTitles: [],
  };
}

function candidateAnswer(orderIndexes: number[]) {
  return JSON.stringify({
    rubric_points: orderIndexes.map((orderIndex) => ({
      source_public_id: `candidate-${orderIndex}`,
      stable_id: `candidate-stable-${orderIndex}`,
      order_index: orderIndex,
      canonical_text: `候选评分点 ${orderIndex + 1}`,
      max_score: 2,
    })),
  });
}

function reviewItem(
  assessmentItemId: number,
  orderIndexes: number[],
  boundOrderIndexes: number[],
  overrides: Partial<AnswerSourceReviewItem> = {},
): AnswerSourceReviewItem {
  return {
    assessmentItemId,
    orderIndex: 0,
    questionNo: "1",
    questionType: "short_answer",
    questionStem: "概括历史事件的影响",
    boundAnswerKeyVersionId: 11,
    boundAnswerJson: JSON.stringify({ reference_answer: "旧答案" }),
    boundRubricVersionId: 12,
    boundLinkSetId: 13,
    boundRubricPoints: boundOrderIndexes.map((orderIndex) => boundPoint(`old-${orderIndex}`, orderIndex)),
    candidateId: 21,
    candidateAnswerJson: candidateAnswer(orderIndexes),
    sourceAnchorJson: null,
    matchState: "conflict",
    ...overrides,
  };
}

function review(
  items: AnswerSourceReviewItem[],
  overrides: Partial<AnswerSourceReviewSummary> = {},
): AnswerSourceReviewSummary {
  return {
    ingestBatchId: 31,
    sourceAiRunId: 32,
    sourceState: "needs_review",
    route: "blocked",
    matchedCount: 0,
    conflictCount: items.filter((item) => item.matchState === "conflict").length,
    missingCount: items.filter((item) => item.matchState === "missing").length,
    resolution: null,
    adoption: null,
    items,
    ...overrides,
  };
}

function analysis(resultReview: AnswerSourceReviewSummary | null): AnswerSourceAnalysisResult {
  return {
    run: {
      ai_run_id: 32,
      status: "succeeded",
      output: {
        state: resultReview?.sourceState ?? "ready",
        confidence: 0.92,
        issue_codes: [],
      },
      failure: null,
    },
    review: resultReview,
  };
}

function fixedResult(overrides: Partial<FixedIntakeResult> = {}): FixedIntakeResult {
  return {
    batchId: 101,
    batchPublicId: "batch-101",
    documents: [],
    studentDocumentCount: 6,
    studentPageCount: 12,
    answerDocumentCount: 0,
    route: "ready_for_batch_confirm",
    targetCount: 3,
    readyCount: 0,
    reviewCount: 3,
    blockedCount: 0,
    completedCount: 0,
    reasonCodes: ["MATERIAL_TYPE_CONFIRMATION_REQUIRED"],
    orderPolicy: "natural_filename_then_capture_time",
    orderConfidence: 0.97,
    orderConflictCodes: [],
    materialType: "unknown",
    materialTypeDecision: "suggested",
    materialTypeConfidence: 0.55,
    materialTypeNeedsConfirmation: true,
    groupingRoute: "preview_ready",
    studentGroupCount: 3,
    groupingIssueCodes: [],
    expectedPagesPerAttempt: 2,
    pageCycleSource: "visual_repeating_layout_v1",
    pageCycleConfidence: 0.96,
    pageCycleNeedsTeacherInput: false,
    groupingRoster: [
      { studentId: 21, studentNo: "1", studentName: "甲同学" },
      { studentId: 22, studentNo: "2", studentName: "乙同学" },
    ],
    groupingConfirmed: false,
    groupingFirstStudentNo: null,
    groupingLastStudentNo: null,
    qualityReviewCompleted: false,
    mappedGroupCount: 0,
    rejectedGroupCount: 0,
    nextAction: "确认材料类型",
    ...overrides,
  };
}

function groupedEvidence(pageId = 301): GroupedPageEvidence[] {
  return [{
    groupIndex: 0,
    studentId: 21,
    studentNo: "1",
    studentName: "甲同学",
    pages: [{
      pageId,
      replacedPageId: null,
      pageNo: 1,
      importIndex: 0,
      archivedPath: `/tmp/${pageId}.jpg`,
      originalName: `${pageId}.jpg`,
      pageState: "prepared",
      qualityResult: null,
      matchDecision: "suggested",
    }],
  }];
}

function ordinaryRun(
  pageId: number,
  state: "ready" | "needs_review" | "blocked" = "ready",
): OrdinaryPaperRunResult {
  return {
    ai_run_id: 8000 + pageId,
    status: "succeeded",
    output: {
      schema_version: 1,
      page_id: pageId,
      expected_page_no: 1,
      state,
      quality: { result: "pass", issue_codes: [] },
      alignment: { confidence: 0.99 },
      regions: [],
      printed_questions: [],
      confidence: 0.99,
      issue_codes: [],
    },
    failure: null,
  };
}

function ordinaryConfirmation(pageId: number): OrdinaryStructureConfirmationResult {
  return {
    confirmation: {
      id: 6000 + pageId,
      ai_run_id: 8000 + pageId,
      page_id: pageId,
      alignment_revision_id: 6100 + pageId,
      region_revision_ids: [],
      confirmed_by: "local_teacher",
      created_at: "2026-08-02T12:10:00Z",
    },
    alignment: { id: 6100 + pageId, decision: "teacher_confirmed" },
    regions: [],
  };
}

function answerSheetStatus(ready = false): AnswerSheetTemplateStatus {
  return {
    assessmentVersionId: 101,
    pageNo: 1,
    activeTemplate: null,
    templateSet: {
      assessment_version_id: 101,
      template_version: "v1",
      ready,
      template_set_hash: ready ? "answer-sheet-template-set" : null,
      pages: [{
        page_no: 1,
        expected_item_count: 2,
        objective_item_count: 1,
        subjective_item_count: 1,
        active_template_revision_id: ready ? 71 : null,
        ready,
        issue_codes: [],
      }],
      issue_codes: [],
    },
  };
}

function answerSheetRun(aiRunId = 701): AnswerSheetTemplateRunResult {
  return {
    ai_run_id: aiRunId,
    status: "succeeded",
    output: {
      state: "ready",
      page_no: 1,
      canvas_width: 1000,
      canvas_height: 1400,
      alignment_mode: "printed_anchors",
      anchors: [],
      items: [],
      subjective_regions: [],
      confidence: 0.99,
      issue_codes: [],
    },
    failure: null,
  };
}

function answerSheetPageResult(pageId: number): AnswerSheetPageProcessingResult {
  return {
    structure: {
      materialization: {
        id: 9000 + pageId,
        page_id: pageId,
        template_revision_id: 71,
        alignment_revision_id: 9100 + pageId,
        region_revision_ids: [],
      },
      regions: [],
      routes: [],
    },
    observations: [],
    subjectiveRegions: [],
    subjectiveTranscriptions: [],
    subjectiveFailures: [],
  };
}

function dictationTemplateRevision() {
  return {
    id: 81,
    assessment_version_id: 101,
    revision: 1,
    template_version: "v1",
    page_no: 1,
    source_ai_run_id: 801,
    state: "active" as const,
  };
}

function dictationStatus(active = false): DictationTemplateStatus {
  return {
    assessmentVersionId: 101,
    pageNo: 1,
    activeTemplate: active ? dictationTemplateRevision() : null,
  };
}

function dictationRun(aiRunId = 801): DictationTemplateRunResult {
  return {
    ai_run_id: aiRunId,
    status: "succeeded",
    output: {
      state: "ready",
      page_no: 1,
      canvas_width: 1000,
      canvas_height: 1400,
      regions: [],
      confidence: 0.99,
      issue_codes: [],
    },
    failure: null,
  };
}

function dictationPageResult(pageId: number): DictationPageProcessingResult {
  return {
    structure: {
      materialization: {
        id: 10000 + pageId,
        page_id: pageId,
        template_revision_id: 81,
        alignment_revision_id: 10100 + pageId,
        region_revision_ids: [],
      },
      regions: [],
    },
    transcriptions: [],
  };
}

test("initial answer source state is empty and does not share mapping objects", () => {
  const first = createInitialAnswerSourceState();
  const second = createInitialAnswerSourceState();

  assert.deepEqual(first, {
    analysis: null,
    busy: false,
    error: "",
    rubricPointMappings: {},
  });
  assert.notStrictEqual(first.rubricPointMappings, second.rubricPointMappings);
});

test("ANSWER_SOURCE_STARTED clears the error and preserves retry evidence", () => {
  const oldAnalysis = analysis(review([reviewItem(101, [0], [0])]));
  const previous = {
    analysis: oldAnalysis,
    busy: false,
    error: "上次失败",
    rubricPointMappings: { "101:0": "old-0" },
  };

  const next = fixedIntakeAnswerSourceReducer(previous, { type: "ANSWER_SOURCE_STARTED" });

  assert.equal(next.busy, true);
  assert.equal(next.error, "");
  assert.strictEqual(next.analysis, oldAnalysis);
  assert.strictEqual(next.rubricPointMappings, previous.rubricPointMappings);
});

test("ANSWER_SOURCE_SUCCEEDED stores analysis and pre-fills same-shape rubric mappings", () => {
  const resultReview = review([
    reviewItem(101, [0, 1], [0, 1]),
    reviewItem(102, [0], [0], { matchState: "matched" }),
  ]);
  const resultAnalysis = analysis(resultReview);
  const started = fixedIntakeAnswerSourceReducer(
    createInitialAnswerSourceState(),
    { type: "ANSWER_SOURCE_STARTED" },
  );

  const next = fixedIntakeAnswerSourceReducer(started, {
    type: "ANSWER_SOURCE_SUCCEEDED",
    analysis: resultAnalysis,
  });

  assert.strictEqual(next.analysis, resultAnalysis);
  assert.equal(next.busy, true);
  assert.deepEqual(next.rubricPointMappings, {
    "101:0": "old-0",
    "101:1": "old-1",
  });
});

test("changed rubric shapes require complete mappings and allow multiple new points", () => {
  const resultReview = review([reviewItem(103, [0, 1, 2], [0, 1])]);
  const mappings = initialRubricPointMappings(resultReview);

  assert.deepEqual(mappings, {
    "103:0": "",
    "103:1": "",
    "103:2": "",
  });
  assert.equal(rubricPointMappingsReady(resultReview, mappings), false);
  assert.equal(rubricPointMappingsReady(resultReview, {
    "103:0": "old-0",
    "103:1": NEW_RUBRIC_POINT,
    "103:2": NEW_RUBRIC_POINT,
  }), true);
});

test("one old rubric point cannot be reused twice within the same question", () => {
  const resultReview = review([reviewItem(103, [0, 1, 2], [0, 1])]);

  assert.equal(rubricPointMappingsReady(resultReview, {
    "103:0": "old-0",
    "103:1": "old-0",
    "103:2": NEW_RUBRIC_POINT,
  }), false);

  const twoQuestions = review([
    reviewItem(108, [0], [0]),
    reviewItem(109, [0], [0]),
  ]);
  assert.equal(rubricPointMappingsReady(twoQuestions, {
    "108:0": "old-0",
    "109:0": "old-0",
  }), true);
});

test("ANSWER_SOURCE_FAILED preserves analysis and mappings until FINISHED releases busy", () => {
  const oldAnalysis = analysis(review([reviewItem(104, [0], [0])]));
  const previous = {
    analysis: oldAnalysis,
    busy: true,
    error: "",
    rubricPointMappings: { "104:0": "old-0" },
  };

  const failed = fixedIntakeAnswerSourceReducer(previous, {
    type: "ANSWER_SOURCE_FAILED",
    error: "答案资料识别失败",
  });
  assert.equal(failed.busy, true);
  assert.equal(failed.error, "答案资料识别失败");
  assert.strictEqual(failed.analysis, oldAnalysis);
  assert.strictEqual(failed.rubricPointMappings, previous.rubricPointMappings);

  const finished = fixedIntakeAnswerSourceReducer(failed, { type: "ANSWER_SOURCE_FINISHED" });
  assert.equal(finished.busy, false);
  assert.equal(finished.error, "答案资料识别失败");
  assert.strictEqual(finished.analysis, oldAnalysis);
});

test("ANSWER_SOURCE_RESOLUTION_STARTED preserves the prior error and evidence", () => {
  const oldAnalysis = analysis(review([reviewItem(105, [0], [0])]));
  const previous = {
    analysis: oldAnalysis,
    busy: false,
    error: "上次分析失败",
    rubricPointMappings: { "105:0": "old-0" },
  };

  const next = fixedIntakeAnswerSourceReducer(previous, {
    type: "ANSWER_SOURCE_RESOLUTION_STARTED",
  });

  assert.equal(next.busy, true);
  assert.equal(next.error, "上次分析失败");
  assert.strictEqual(next.analysis, oldAnalysis);
  assert.strictEqual(next.rubricPointMappings, previous.rubricPointMappings);
});

test("ANSWER_SOURCE_RESOLUTION_SUCCEEDED only replaces review and fails closed without analysis", () => {
  const oldReview = review([reviewItem(105, [0], [0])]);
  const oldAnalysis = analysis(oldReview);
  const mappings = { "105:0": "old-0" };
  const previous = {
    analysis: oldAnalysis,
    busy: true,
    error: "",
    rubricPointMappings: mappings,
  };
  const confirmedReview = review(oldReview.items, {
    route: "confirmed",
    sourceState: "ready",
    resolution: "confirmed_matches",
  });

  const next = fixedIntakeAnswerSourceReducer(previous, {
    type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED",
    review: confirmedReview,
  });
  assert.strictEqual(next.analysis?.run, oldAnalysis.run);
  assert.strictEqual(next.analysis?.review, confirmedReview);
  assert.strictEqual(next.rubricPointMappings, mappings);
  assert.equal(next.busy, true);

  const empty = createInitialAnswerSourceState();
  assert.strictEqual(fixedIntakeAnswerSourceReducer(empty, {
    type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED",
    review: confirmedReview,
  }), empty);
});

test("RUBRIC_MAPPING_CHANGED updates one mapping without mutating prior state", () => {
  const previous = {
    ...createInitialAnswerSourceState(),
    rubricPointMappings: {
      "106:0": "old-0",
      "106:1": "old-1",
    },
  };

  const next = fixedIntakeAnswerSourceReducer(previous, {
    type: "RUBRIC_MAPPING_CHANGED",
    assessmentItemId: 106,
    orderIndex: 1,
    stableId: NEW_RUBRIC_POINT,
  });

  assert.equal(rubricPointMappingKey(106, 1), "106:1");
  assert.deepEqual(next.rubricPointMappings, {
    "106:0": "old-0",
    "106:1": NEW_RUBRIC_POINT,
  });
  assert.deepEqual(previous.rubricPointMappings, {
    "106:0": "old-0",
    "106:1": "old-1",
  });
  assert.notStrictEqual(next.rubricPointMappings, previous.rubricPointMappings);
});

test("ANSWER_SOURCE_RESET clears the slice with a fresh mapping object", () => {
  const previous = {
    analysis: analysis(review([reviewItem(107, [0], [0])])),
    busy: true,
    error: "旧错误",
    rubricPointMappings: { "107:0": "old-0" },
  };

  const next = fixedIntakeAnswerSourceReducer(previous, { type: "ANSWER_SOURCE_RESET" });

  assert.deepEqual(next, createInitialAnswerSourceState());
  assert.notStrictEqual(next.rubricPointMappings, previous.rubricPointMappings);
});

test("batch workflow initial state owns the supplied context and fresh draft arrays", () => {
  const first = createInitialFixedIntakeBatchWorkflowState({
    classId: 1,
    assessmentVersionId: 12,
  });
  const second = createInitialFixedIntakeBatchWorkflowState();

  assert.deepEqual(first, {
    context: { classId: 1, assessmentVersionId: 12 },
    draft: {
      studentPaths: [],
      answerPath: null,
      answerText: "",
      expectedPages: "1",
      pageCycle: null,
    },
    batch: { requestKey: "", busy: false, result: null, confirmingType: false },
    grouping: {
      startNo: "",
      absentStudentNos: [],
      confirming: false,
      evidence: [],
      loadingEvidence: false,
    },
    quality: { rejectedPageIds: [], confirming: false, retakingPageId: null },
    ordinary: { runs: {}, analyzingPageIds: [], confirmations: {}, confirmingPageIds: [] },
    answerSheet: {
      templateStatus: null,
      templateStatusLoaded: false,
      templateRun: null,
      templateBusy: false,
      pageResults: {},
      pageFailures: {},
      processingPageIds: [],
    },
    dictation: {
      templateStatus: null,
      templateStatusLoaded: false,
      templateRun: null,
      templateBusy: false,
      pageResults: {},
      pageFailures: {},
      processingPageIds: [],
    },
  });
  assert.deepEqual(second.context, { classId: 0, assessmentVersionId: 0 });
  assert.notStrictEqual(first.draft.studentPaths, second.draft.studentPaths);
  assert.notStrictEqual(first.grouping.absentStudentNos, second.grouping.absentStudentNos);
  assert.notStrictEqual(first.grouping.evidence, second.grouping.evidence);
  assert.notStrictEqual(first.quality.rejectedPageIds, second.quality.rejectedPageIds);
  assert.notStrictEqual(first.ordinary.runs, second.ordinary.runs);
  assert.notStrictEqual(first.ordinary.analyzingPageIds, second.ordinary.analyzingPageIds);
  assert.notStrictEqual(first.ordinary.confirmations, second.ordinary.confirmations);
  assert.notStrictEqual(first.ordinary.confirmingPageIds, second.ordinary.confirmingPageIds);
  assert.notStrictEqual(first.answerSheet.pageResults, second.answerSheet.pageResults);
  assert.notStrictEqual(first.answerSheet.pageFailures, second.answerSheet.pageFailures);
  assert.notStrictEqual(first.answerSheet.processingPageIds, second.answerSheet.processingPageIds);
  assert.notStrictEqual(first.dictation.pageResults, second.dictation.pageResults);
  assert.notStrictEqual(first.dictation.pageFailures, second.dictation.pageFailures);
  assert.notStrictEqual(first.dictation.processingPageIds, second.dictation.processingPageIds);
});

test("context actions replace only one selection and return the same state for no-op values", () => {
  const initial = createInitialFixedIntakeBatchWorkflowState({
    classId: 1,
    assessmentVersionId: 12,
  });

  assert.strictEqual(fixedIntakeBatchWorkflowReducer(initial, {
    type: "CONTEXT_CLASS_CHANGED",
    classId: 1,
  }), initial);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(initial, {
    type: "CONTEXT_ASSESSMENT_CHANGED",
    assessmentVersionId: 12,
  }), initial);

  const classChanged = fixedIntakeBatchWorkflowReducer(initial, {
    type: "CONTEXT_CLASS_CHANGED",
    classId: 2,
  });
  assert.deepEqual(classChanged.context, { classId: 2, assessmentVersionId: 12 });
  assert.strictEqual(classChanged.draft, initial.draft);
  assert.strictEqual(classChanged.batch, initial.batch);
  assert.strictEqual(classChanged.grouping, initial.grouping);
  assert.strictEqual(classChanged.quality, initial.quality);
  assert.strictEqual(classChanged.ordinary, initial.ordinary);
  assert.strictEqual(classChanged.answerSheet, initial.answerSheet);
  assert.strictEqual(classChanged.dictation, initial.dictation);

  const assessmentChanged = fixedIntakeBatchWorkflowReducer(classChanged, {
    type: "CONTEXT_ASSESSMENT_CHANGED",
    assessmentVersionId: 22,
  });
  assert.deepEqual(assessmentChanged.context, { classId: 2, assessmentVersionId: 22 });
  assert.strictEqual(assessmentChanged.draft, initial.draft);
  assert.strictEqual(assessmentChanged.batch, initial.batch);
});

test("draft actions replace only their field and preserve every processing slice", () => {
  const initial = createInitialFixedIntakeBatchWorkflowState({
    classId: 1,
    assessmentVersionId: 12,
  });
  const studentPaths = ["/tmp/IMG_0001.jpg", "/tmp/IMG_0002.jpg"];
  const pageCycle: PageCycleSuggestion = {
    expectedPagesPerAttempt: 2,
    confidence: 0.96,
    source: "visual_repeating_layout_v1",
    issueCodes: [],
    needsTeacherInput: false,
  };
  const actions = [
    { type: "DRAFT_STUDENT_PATHS_CHANGED" as const, studentPaths },
    { type: "DRAFT_ANSWER_PATH_CHANGED" as const, answerPath: "/tmp/答案.docx" },
    { type: "DRAFT_ANSWER_TEXT_CHANGED" as const, answerText: "第1题 B" },
    { type: "DRAFT_EXPECTED_PAGES_CHANGED" as const, expectedPages: "2" },
    { type: "DRAFT_PAGE_CYCLE_RESOLVED" as const, pageCycle },
  ];
  let current = initial;
  for (const action of actions) {
    const previous = current;
    current = fixedIntakeBatchWorkflowReducer(current, action);
    assert.notStrictEqual(current, previous);
    assert.notStrictEqual(current.draft, previous.draft);
    assert.strictEqual(current.context, initial.context);
    assert.strictEqual(current.batch, initial.batch);
    assert.strictEqual(current.grouping, initial.grouping);
    assert.strictEqual(current.quality, initial.quality);
    assert.strictEqual(current.ordinary, initial.ordinary);
    assert.strictEqual(current.answerSheet, initial.answerSheet);
    assert.strictEqual(current.dictation, initial.dictation);
  }
  assert.deepEqual(current.draft, {
    studentPaths,
    answerPath: "/tmp/答案.docx",
    answerText: "第1题 B",
    expectedPages: "2",
    pageCycle,
  });
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(current, {
    type: "DRAFT_STUDENT_PATHS_CHANGED",
    studentPaths,
  }), current);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(current, {
    type: "DRAFT_ANSWER_PATH_CHANGED",
    answerPath: "/tmp/答案.docx",
  }), current);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(current, {
    type: "DRAFT_ANSWER_TEXT_CHANGED",
    answerText: "第1题 B",
  }), current);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(current, {
    type: "DRAFT_EXPECTED_PAGES_CHANGED",
    expectedPages: "2",
  }), current);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(current, {
    type: "DRAFT_PAGE_CYCLE_RESOLVED",
    pageCycle,
  }), current);
});

test("session reset clears only the legacy reset fields and preserves operation flags", () => {
  const evidence = groupedEvidence();
  const previous = {
    context: { classId: 1, assessmentVersionId: 12 },
    draft: {
      studentPaths: ["/tmp/IMG_0001.jpg"],
      answerPath: null,
      answerText: "第1题 B",
      expectedPages: "2",
      pageCycle: null,
    },
    batch: {
      requestKey: "prepare-key",
      busy: true,
      result: fixedResult(),
      confirmingType: true,
    },
    grouping: {
      startNo: "2",
      absentStudentNos: ["3"],
      confirming: true,
      evidence,
      loadingEvidence: true,
    },
    quality: {
      rejectedPageIds: [301],
      confirming: true,
      retakingPageId: 301,
    },
    ordinary: {
      runs: { 301: ordinaryRun(301) },
      analyzingPageIds: [301],
      confirmations: { 301: ordinaryConfirmation(301) },
      confirmingPageIds: [301],
    },
    answerSheet: {
      templateStatus: answerSheetStatus(true),
      templateStatusLoaded: true,
      templateRun: answerSheetRun(),
      templateBusy: true,
      pageResults: { 301: answerSheetPageResult(301) },
      pageFailures: { 302: "旧失败" },
      processingPageIds: [303],
    },
    dictation: {
      templateStatus: dictationStatus(true),
      templateStatusLoaded: true,
      templateRun: dictationRun(),
      templateBusy: true,
      pageResults: { 301: dictationPageResult(301) },
      pageFailures: { 302: "旧失败" },
      processingPageIds: [303],
    },
  };

  const next = fixedIntakeBatchWorkflowReducer(previous, { type: "FIXED_INTAKE_SESSION_RESET" });

  assert.deepEqual(next, {
    context: previous.context,
    draft: previous.draft,
    batch: { requestKey: "", busy: true, result: null, confirmingType: true },
    grouping: {
      startNo: "",
      absentStudentNos: [],
      confirming: true,
      evidence: [],
      loadingEvidence: true,
    },
    quality: { rejectedPageIds: [], confirming: true, retakingPageId: 301 },
    ordinary: previous.ordinary,
    answerSheet: previous.answerSheet,
    dictation: previous.dictation,
  });
  assert.notStrictEqual(next.grouping.absentStudentNos, previous.grouping.absentStudentNos);
  assert.notStrictEqual(next.grouping.evidence, evidence);
  assert.notStrictEqual(next.quality.rejectedPageIds, previous.quality.rejectedPageIds);
  assert.strictEqual(next.context, previous.context);
  assert.strictEqual(next.draft, previous.draft);
  assert.strictEqual(next.ordinary, previous.ordinary);
  assert.strictEqual(next.answerSheet, previous.answerSheet);
  assert.strictEqual(next.dictation, previous.dictation);
});

test("prepare invalidation and lifecycle preserve retry key and existing review projections", () => {
  const previous = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    batch: {
      requestKey: "old-key",
      busy: false,
      result: fixedResult(),
      confirmingType: false,
    },
    grouping: {
      ...createInitialFixedIntakeBatchWorkflowState().grouping,
      startNo: "2",
      absentStudentNos: ["3"],
      evidence: groupedEvidence(),
    },
    quality: {
      ...createInitialFixedIntakeBatchWorkflowState().quality,
      rejectedPageIds: [301],
    },
  };

  const invalidated = fixedIntakeBatchWorkflowReducer(previous, {
    type: "PREPARE_INPUT_INVALIDATED",
  });
  assert.equal(invalidated.batch.requestKey, "");
  assert.equal(invalidated.batch.result, null);
  assert.strictEqual(invalidated.context, previous.context);
  assert.strictEqual(invalidated.draft, previous.draft);
  assert.strictEqual(invalidated.grouping, previous.grouping);
  assert.strictEqual(invalidated.quality, previous.quality);

  const started = fixedIntakeBatchWorkflowReducer(previous, {
    type: "PREPARE_STARTED",
    requestKey: "old-key",
  });
  assert.equal(started.batch.requestKey, "old-key");
  assert.equal(started.batch.busy, true);
  assert.strictEqual(started.context, previous.context);
  assert.strictEqual(started.draft, previous.draft);

  const prepared = fixedResult({ groupingFirstStudentNo: null });
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "PREPARE_SUCCEEDED",
    result: prepared,
  });
  assert.strictEqual(succeeded.batch.result, prepared);
  assert.equal(succeeded.batch.requestKey, "");
  assert.equal(succeeded.batch.busy, true);
  assert.equal(succeeded.grouping.startNo, "1");
  assert.deepEqual(succeeded.grouping.absentStudentNos, []);
  assert.strictEqual(succeeded.grouping.evidence, previous.grouping.evidence);
  assert.strictEqual(succeeded.quality.rejectedPageIds, previous.quality.rejectedPageIds);
  assert.strictEqual(succeeded.context, previous.context);
  assert.strictEqual(succeeded.draft, previous.draft);

  const finished = fixedIntakeBatchWorkflowReducer(succeeded, { type: "PREPARE_FINISHED" });
  assert.equal(finished.batch.busy, false);
  assert.strictEqual(finished.batch.result, prepared);
  assert.strictEqual(finished.context, previous.context);
  assert.strictEqual(finished.draft, previous.draft);
});

test("answer source reason replacement only changes answer reason codes", () => {
  const previous = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    batch: {
      ...createInitialFixedIntakeBatchWorkflowState().batch,
      result: fixedResult({
        reasonCodes: [
          "MATERIAL_TYPE_CONFIRMATION_REQUIRED",
          "ANSWER_SOURCE_STRUCTURE_PENDING",
          "ANSWER_SOURCE_CONFLICT_OR_MISSING",
        ],
      }),
    },
  };

  const next = fixedIntakeBatchWorkflowReducer(previous, {
    type: "ANSWER_SOURCE_REASON_REPLACED",
    reasonCode: "ANSWER_SOURCE_CONFIRMATION_REQUIRED",
  });
  assert.deepEqual(next.batch.result?.reasonCodes, [
    "MATERIAL_TYPE_CONFIRMATION_REQUIRED",
    "ANSWER_SOURCE_CONFIRMATION_REQUIRED",
  ]);

  const empty = createInitialFixedIntakeBatchWorkflowState();
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(empty, {
    type: "ANSWER_SOURCE_REASON_REPLACED",
    reasonCode: null,
  }), empty);
});

test("material and grouping confirmation transitions keep request semantics", () => {
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    batch: {
      ...createInitialFixedIntakeBatchWorkflowState().batch,
      result: fixedResult(),
    },
    grouping: {
      ...createInitialFixedIntakeBatchWorkflowState().grouping,
      evidence: groupedEvidence(),
    },
    quality: {
      ...createInitialFixedIntakeBatchWorkflowState().quality,
      rejectedPageIds: [301],
    },
  };

  const materialStarted = fixedIntakeBatchWorkflowReducer(initial, {
    type: "MATERIAL_CONFIRMATION_STARTED",
  });
  assert.equal(materialStarted.batch.confirmingType, true);
  const materialSucceeded = fixedIntakeBatchWorkflowReducer(materialStarted, {
    type: "MATERIAL_CONFIRMATION_SUCCEEDED",
    confirmation: {
      materialType: "ordinary_paper",
      materialTypeDecision: "teacher_confirmed",
      materialTypeConfidence: 1,
      groupingRoute: "preview_ready",
      studentGroupCount: 3,
      groupingIssueCodes: [],
      nextAction: "确认学生顺序",
    },
  });
  assert.equal(materialSucceeded.batch.result?.materialType, "ordinary_paper");
  assert.equal(materialSucceeded.batch.result?.materialTypeNeedsConfirmation, false);
  const materialFinished = fixedIntakeBatchWorkflowReducer(materialSucceeded, {
    type: "MATERIAL_CONFIRMATION_FINISHED",
  });
  assert.equal(materialFinished.batch.confirmingType, false);

  const groupingStarted = fixedIntakeBatchWorkflowReducer(materialFinished, {
    type: "GROUPING_CONFIRMATION_STARTED",
  });
  assert.equal(groupingStarted.grouping.confirming, true);
  const groupingSucceeded = fixedIntakeBatchWorkflowReducer(groupingStarted, {
    type: "GROUPING_CONFIRMATION_SUCCEEDED",
    confirmation: {
      groupingRoute: "preview_ready",
      studentGroupCount: 3,
      groupingIssueCodes: [],
      groupingConfirmed: true,
      groupingFirstStudentNo: "1",
      groupingLastStudentNo: "4",
      nextAction: "检查照片质量",
    },
  });
  assert.equal(groupingSucceeded.batch.result?.groupingConfirmed, true);
  assert.deepEqual(groupingSucceeded.grouping.evidence, []);
  assert.deepEqual(groupingSucceeded.quality.rejectedPageIds, []);
  assert.equal(groupingSucceeded.grouping.confirming, true);
  const groupingFinished = fixedIntakeBatchWorkflowReducer(groupingSucceeded, {
    type: "GROUPING_CONFIRMATION_FINISHED",
  });
  assert.equal(groupingFinished.grouping.confirming, false);
});

test("grouping draft and evidence actions are immutable and failure-friendly", () => {
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    grouping: {
      ...createInitialFixedIntakeBatchWorkflowState().grouping,
      startNo: "1",
      absentStudentNos: ["3"],
      evidence: groupedEvidence(),
    },
  };

  const changedStart = fixedIntakeBatchWorkflowReducer(initial, {
    type: "GROUPING_START_CHANGED",
    studentNo: "2",
  });
  assert.equal(changedStart.grouping.startNo, "2");
  assert.deepEqual(changedStart.grouping.absentStudentNos, []);
  assert.deepEqual(initial.grouping.absentStudentNos, ["3"]);

  const added = fixedIntakeBatchWorkflowReducer(changedStart, {
    type: "ABSENT_STUDENT_TOGGLED",
    studentNo: "4",
  });
  assert.deepEqual(added.grouping.absentStudentNos, ["4"]);
  const removed = fixedIntakeBatchWorkflowReducer(added, {
    type: "ABSENT_STUDENT_TOGGLED",
    studentNo: "4",
  });
  assert.deepEqual(removed.grouping.absentStudentNos, []);

  const loading = fixedIntakeBatchWorkflowReducer(initial, { type: "GROUPING_EVIDENCE_STARTED" });
  assert.equal(loading.grouping.loadingEvidence, true);
  const failedFinished = fixedIntakeBatchWorkflowReducer(loading, { type: "GROUPING_EVIDENCE_FINISHED" });
  assert.equal(failedFinished.grouping.loadingEvidence, false);
  assert.strictEqual(failedFinished.grouping.evidence, initial.grouping.evidence);

  const replacement = groupedEvidence(401);
  const succeeded = fixedIntakeBatchWorkflowReducer(loading, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence: replacement,
  });
  assert.strictEqual(succeeded.grouping.evidence, replacement);
});

test("quality rejection and confirmation preserve teacher draft boundaries", () => {
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    batch: {
      ...createInitialFixedIntakeBatchWorkflowState().batch,
      result: fixedResult(),
    },
  };
  const rejected = fixedIntakeBatchWorkflowReducer(initial, {
    type: "QUALITY_REJECTION_TOGGLED",
    pageId: 301,
  });
  assert.deepEqual(rejected.quality.rejectedPageIds, [301]);
  assert.deepEqual(initial.quality.rejectedPageIds, []);

  const confirming = fixedIntakeBatchWorkflowReducer(rejected, {
    type: "QUALITY_CONFIRMATION_STARTED",
  });
  assert.equal(confirming.quality.confirming, true);
  const confirmed = fixedIntakeBatchWorkflowReducer(confirming, {
    type: "QUALITY_CONFIRMATION_SUCCEEDED",
    confirmation: {
      qualityReviewCompleted: true,
      mappedGroupCount: 2,
      rejectedGroupCount: 1,
      nextAction: "补拍异常页",
    },
  });
  assert.equal(confirmed.batch.result?.qualityReviewCompleted, true);
  assert.deepEqual(confirmed.quality.rejectedPageIds, [301]);
  const finished = fixedIntakeBatchWorkflowReducer(confirmed, {
    type: "QUALITY_CONFIRMATION_FINISHED",
  });
  assert.equal(finished.quality.confirming, false);

  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "QUALITY_REJECTION_TOGGLED",
    pageId: 302,
  }), finished);
});

test("retake lifecycle updates only current summary and keeps review evidence", () => {
  const evidence = groupedEvidence();
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    batch: {
      ...createInitialFixedIntakeBatchWorkflowState().batch,
      result: fixedResult({ qualityReviewCompleted: true, mappedGroupCount: 2, rejectedGroupCount: 1 }),
    },
    grouping: {
      ...createInitialFixedIntakeBatchWorkflowState().grouping,
      evidence,
    },
    quality: {
      ...createInitialFixedIntakeBatchWorkflowState().quality,
      rejectedPageIds: [301],
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, { type: "RETAKE_STARTED", pageId: 301 });
  assert.equal(started.quality.retakingPageId, 301);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "RETAKE_SUCCEEDED",
    replacement: {
      replacementPageId: 1301,
      activatedStudent: true,
      mappedGroupCount: 3,
      rejectedGroupCount: 0,
      nextAction: "进入识别",
    },
  });
  assert.equal(succeeded.batch.result?.mappedGroupCount, 3);
  assert.equal(succeeded.batch.result?.rejectedGroupCount, 0);
  assert.equal(succeeded.batch.result?.qualityReviewCompleted, true);
  assert.strictEqual(succeeded.grouping.evidence, evidence);
  assert.deepEqual(succeeded.quality.rejectedPageIds, [301]);

  const finished = fixedIntakeBatchWorkflowReducer(succeeded, { type: "RETAKE_FINISHED" });
  assert.equal(finished.quality.retakingPageId, null);
});

test("result-dependent success events fail closed when no batch result exists", () => {
  const empty = createInitialFixedIntakeBatchWorkflowState();

  assert.strictEqual(fixedIntakeBatchWorkflowReducer(empty, {
    type: "MATERIAL_CONFIRMATION_SUCCEEDED",
    confirmation: {
      materialType: "ordinary_paper",
      materialTypeDecision: "teacher_confirmed",
      materialTypeConfidence: 1,
      groupingRoute: "preview_ready",
      studentGroupCount: 1,
      groupingIssueCodes: [],
      nextAction: "确认学生顺序",
    },
  }), empty);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(empty, {
    type: "GROUPING_CONFIRMATION_SUCCEEDED",
    confirmation: {
      groupingRoute: "preview_ready",
      studentGroupCount: 1,
      groupingIssueCodes: [],
      groupingConfirmed: true,
      groupingFirstStudentNo: "1",
      groupingLastStudentNo: "1",
      nextAction: "检查照片质量",
    },
  }), empty);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(empty, {
    type: "QUALITY_CONFIRMATION_SUCCEEDED",
    confirmation: {
      qualityReviewCompleted: true,
      mappedGroupCount: 1,
      rejectedGroupCount: 0,
      nextAction: "进入识别",
    },
  }), empty);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(empty, {
    type: "RETAKE_SUCCEEDED",
    replacement: {
      replacementPageId: 1301,
      activatedStudent: true,
      mappedGroupCount: 1,
      rejectedGroupCount: 0,
      nextAction: "进入识别",
    },
  }), empty);
});

test("ordinary partial and full reset preserve the two legacy reset scopes", () => {
  const ordinary = {
    runs: { 301: ordinaryRun(301) },
    analyzingPageIds: [301, 302],
    confirmations: { 301: ordinaryConfirmation(301) },
    confirmingPageIds: [301],
  };
  const initial = { ...createInitialFixedIntakeBatchWorkflowState(), ordinary };

  const partial = fixedIntakeBatchWorkflowReducer(initial, { type: "ORDINARY_PARTIAL_RESET" });
  assert.deepEqual(partial.ordinary.runs, {});
  assert.deepEqual(partial.ordinary.analyzingPageIds, []);
  assert.strictEqual(partial.ordinary.confirmations, ordinary.confirmations);
  assert.strictEqual(partial.ordinary.confirmingPageIds, ordinary.confirmingPageIds);
  assert.strictEqual(partial.batch, initial.batch);

  const full = fixedIntakeBatchWorkflowReducer(initial, { type: "ORDINARY_FULL_RESET" });
  assert.deepEqual(full.ordinary, {
    runs: {},
    analyzingPageIds: [],
    confirmations: {},
    confirmingPageIds: [],
  });
  assert.notStrictEqual(full.ordinary.runs, ordinary.runs);
  assert.notStrictEqual(full.ordinary.confirmations, ordinary.confirmations);
});

test("ordinary analysis lifecycle appends unique pages and stores one run immutably", () => {
  const existing = ordinaryRun(301, "needs_review");
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    ordinary: {
      ...createInitialFixedIntakeBatchWorkflowState().ordinary,
      runs: { 301: existing },
      analyzingPageIds: [301],
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ORDINARY_ANALYSIS_STARTED",
    pageIds: [301, 302, 302],
  });
  assert.deepEqual(started.ordinary.analyzingPageIds, [301, 302]);
  assert.deepEqual(initial.ordinary.analyzingPageIds, [301]);

  const run = ordinaryRun(302);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "ORDINARY_ANALYSIS_SUCCEEDED",
    pageId: 302,
    run,
  });
  assert.strictEqual(succeeded.ordinary.runs[301], existing);
  assert.strictEqual(succeeded.ordinary.runs[302], run);
  assert.deepEqual(initial.ordinary.runs, { 301: existing });

  const finished = fixedIntakeBatchWorkflowReducer(succeeded, {
    type: "ORDINARY_ANALYSIS_FINISHED",
    pageId: 302,
  });
  assert.deepEqual(finished.ordinary.analyzingPageIds, [301]);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "ORDINARY_ANALYSIS_FINISHED",
    pageId: 999,
  }), finished);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "ORDINARY_ANALYSIS_STARTED",
    pageIds: [],
  }), finished);
});

test("ordinary confirmation lifecycle replaces the batch occupancy and stores partial success", () => {
  const existing = ordinaryConfirmation(301);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    ordinary: {
      ...createInitialFixedIntakeBatchWorkflowState().ordinary,
      confirmations: { 301: existing },
      confirmingPageIds: [999],
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ORDINARY_CONFIRMATION_STARTED",
    pageIds: [302, 303],
  });
  assert.deepEqual(started.ordinary.confirmingPageIds, [302, 303]);

  const confirmation = ordinaryConfirmation(302);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "ORDINARY_CONFIRMATION_SUCCEEDED",
    pageId: 302,
    confirmation,
  });
  assert.strictEqual(succeeded.ordinary.confirmations[301], existing);
  assert.strictEqual(succeeded.ordinary.confirmations[302], confirmation);
  assert.deepEqual(started.ordinary.confirmations, { 301: existing });

  const finished = fixedIntakeBatchWorkflowReducer(succeeded, {
    type: "ORDINARY_CONFIRMATION_FINISHED",
    pageId: 302,
  });
  assert.deepEqual(finished.ordinary.confirmingPageIds, [303]);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "ORDINARY_CONFIRMATION_FINISHED",
    pageId: 999,
  }), finished);
});

test("ordinary actions do not mutate batch grouping or quality slices", () => {
  const initial = createInitialFixedIntakeBatchWorkflowState();
  const run = ordinaryRun(301);
  const next = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ORDINARY_ANALYSIS_SUCCEEDED",
    pageId: 301,
    run,
  });

  assert.strictEqual(next.batch, initial.batch);
  assert.strictEqual(next.grouping, initial.grouping);
  assert.strictEqual(next.quality, initial.quality);
  assert.strictEqual(next.ordinary.runs[301], run);
});

test("answer-sheet reset clears the six legacy fields and preserves template busy", () => {
  const pageResult = answerSheetPageResult(301);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    answerSheet: {
      templateStatus: answerSheetStatus(true),
      templateStatusLoaded: true,
      templateRun: answerSheetRun(),
      templateBusy: true,
      pageResults: { 301: pageResult },
      pageFailures: { 302: "待重试" },
      processingPageIds: [303],
    },
  };

  const reset = fixedIntakeBatchWorkflowReducer(initial, { type: "ANSWER_SHEET_RESET" });
  assert.deepEqual(reset.answerSheet, {
    templateStatus: null,
    templateStatusLoaded: false,
    templateRun: null,
    templateBusy: true,
    pageResults: {},
    pageFailures: {},
    processingPageIds: [],
  });
  assert.strictEqual(reset.batch, initial.batch);
  assert.strictEqual(reset.grouping, initial.grouping);
  assert.strictEqual(reset.quality, initial.quality);
  assert.strictEqual(reset.ordinary, initial.ordinary);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(reset, {
    type: "ANSWER_SHEET_RESET",
  }), reset);
});

test("answer-sheet template status loading preserves retry evidence", () => {
  const oldStatus = answerSheetStatus(false);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    answerSheet: {
      ...createInitialFixedIntakeBatchWorkflowState().answerSheet,
      templateStatus: oldStatus,
      templateStatusLoaded: true,
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ANSWER_SHEET_STATUS_LOAD_STARTED",
  });
  assert.equal(started.answerSheet.templateStatusLoaded, false);
  assert.strictEqual(started.answerSheet.templateStatus, oldStatus);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(started, {
    type: "ANSWER_SHEET_STATUS_LOAD_STARTED",
  }), started);

  const refreshed = answerSheetStatus(true);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED",
    status: refreshed,
  });
  assert.equal(succeeded.answerSheet.templateStatusLoaded, true);
  assert.strictEqual(succeeded.answerSheet.templateStatus, refreshed);

  const retried = fixedIntakeBatchWorkflowReducer(succeeded, {
    type: "ANSWER_SHEET_STATUS_LOAD_STARTED",
  });
  const failed = fixedIntakeBatchWorkflowReducer(retried, {
    type: "ANSWER_SHEET_STATUS_LOAD_FAILED",
  });
  assert.equal(failed.answerSheet.templateStatusLoaded, true);
  assert.strictEqual(failed.answerSheet.templateStatus, refreshed);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(failed, {
    type: "ANSWER_SHEET_STATUS_LOAD_FAILED",
  }), failed);
});

test("answer-sheet template analysis and confirmation keep the two request boundaries", () => {
  const oldRun = answerSheetRun(701);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    answerSheet: {
      ...createInitialFixedIntakeBatchWorkflowState().answerSheet,
      templateRun: oldRun,
    },
  };

  const analysisStarted = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_STARTED",
  });
  assert.equal(analysisStarted.answerSheet.templateBusy, true);
  assert.equal(analysisStarted.answerSheet.templateRun, null);

  const returnedRun = answerSheetRun(702);
  const analysisSucceeded = fixedIntakeBatchWorkflowReducer(analysisStarted, {
    type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED",
    run: returnedRun,
  });
  assert.strictEqual(analysisSucceeded.answerSheet.templateRun, returnedRun);
  assert.equal(analysisSucceeded.answerSheet.templateBusy, true);

  const confirmationStarted = fixedIntakeBatchWorkflowReducer(analysisSucceeded, {
    type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_STARTED",
  });
  assert.equal(confirmationStarted.answerSheet.templateBusy, true);
  assert.strictEqual(confirmationStarted.answerSheet.templateRun, returnedRun);

  const refreshed = answerSheetStatus(true);
  const confirmed = fixedIntakeBatchWorkflowReducer(confirmationStarted, {
    type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED",
    status: refreshed,
  });
  assert.strictEqual(confirmed.answerSheet.templateStatus, refreshed);
  assert.equal(confirmed.answerSheet.templateRun, null);
  assert.equal(confirmed.answerSheet.templateBusy, true);

  const finished = fixedIntakeBatchWorkflowReducer(confirmed, {
    type: "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED",
  });
  assert.equal(finished.answerSheet.templateBusy, false);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED",
  }), finished);
});

test("answer-sheet page lifecycle keeps sequential partial success semantics", () => {
  const oldResult = answerSheetPageResult(301);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    answerSheet: {
      ...createInitialFixedIntakeBatchWorkflowState().answerSheet,
      pageResults: { 301: oldResult },
      pageFailures: { 302: "旧失败", 303: "保留失败" },
      processingPageIds: [301],
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ANSWER_SHEET_PAGES_STARTED",
    pageIds: [301, 302, 302],
  });
  assert.deepEqual(started.answerSheet.processingPageIds, [301, 302]);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(started, {
    type: "ANSWER_SHEET_PAGES_STARTED",
    pageIds: [301, 302],
  }), started);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(started, {
    type: "ANSWER_SHEET_PAGES_STARTED",
    pageIds: [],
  }), started);

  const newResult = answerSheetPageResult(302);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "ANSWER_SHEET_PAGE_SUCCEEDED",
    pageId: 302,
    result: newResult,
  });
  assert.strictEqual(succeeded.answerSheet.pageResults[301], oldResult);
  assert.strictEqual(succeeded.answerSheet.pageResults[302], newResult);
  assert.equal(succeeded.answerSheet.pageFailures[302], undefined);
  assert.equal(succeeded.answerSheet.pageFailures[303], "保留失败");
  assert.equal(started.answerSheet.pageFailures[302], "旧失败");

  const failed = fixedIntakeBatchWorkflowReducer(succeeded, {
    type: "ANSWER_SHEET_PAGE_FAILED",
    pageId: 302,
    error: "新失败",
  });
  assert.strictEqual(failed.answerSheet.pageResults[302], newResult);
  assert.equal(failed.answerSheet.pageFailures[302], "新失败");

  const finished = fixedIntakeBatchWorkflowReducer(failed, {
    type: "ANSWER_SHEET_PAGE_FINISHED",
    pageId: 302,
  });
  assert.deepEqual(finished.answerSheet.processingPageIds, [301]);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "ANSWER_SHEET_PAGE_FINISHED",
    pageId: 999,
  }), finished);
});

test("answer-sheet actions do not mutate batch grouping quality or ordinary slices", () => {
  const initial = createInitialFixedIntakeBatchWorkflowState();
  const status = answerSheetStatus(true);
  const next = fixedIntakeBatchWorkflowReducer(initial, {
    type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED",
    status,
  });

  assert.strictEqual(next.batch, initial.batch);
  assert.strictEqual(next.grouping, initial.grouping);
  assert.strictEqual(next.quality, initial.quality);
  assert.strictEqual(next.ordinary, initial.ordinary);
  assert.strictEqual(next.answerSheet.templateStatus, status);
});

test("dictation reset clears the six legacy fields and preserves template busy", () => {
  const pageResult = dictationPageResult(301);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    dictation: {
      templateStatus: dictationStatus(true),
      templateStatusLoaded: true,
      templateRun: dictationRun(),
      templateBusy: true,
      pageResults: { 301: pageResult },
      pageFailures: { 302: "待重试" },
      processingPageIds: [303],
    },
  };

  const reset = fixedIntakeBatchWorkflowReducer(initial, { type: "DICTATION_RESET" });
  assert.deepEqual(reset.dictation, {
    templateStatus: null,
    templateStatusLoaded: false,
    templateRun: null,
    templateBusy: true,
    pageResults: {},
    pageFailures: {},
    processingPageIds: [],
  });
  assert.strictEqual(reset.batch, initial.batch);
  assert.strictEqual(reset.grouping, initial.grouping);
  assert.strictEqual(reset.quality, initial.quality);
  assert.strictEqual(reset.ordinary, initial.ordinary);
  assert.strictEqual(reset.answerSheet, initial.answerSheet);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(reset, {
    type: "DICTATION_RESET",
  }), reset);
});

test("dictation template status loading preserves retry evidence", () => {
  const oldStatus = dictationStatus(false);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    dictation: {
      ...createInitialFixedIntakeBatchWorkflowState().dictation,
      templateStatus: oldStatus,
      templateStatusLoaded: true,
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, {
    type: "DICTATION_STATUS_LOAD_STARTED",
  });
  assert.equal(started.dictation.templateStatusLoaded, false);
  assert.strictEqual(started.dictation.templateStatus, oldStatus);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(started, {
    type: "DICTATION_STATUS_LOAD_STARTED",
  }), started);

  const refreshed = dictationStatus(true);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "DICTATION_STATUS_LOAD_SUCCEEDED",
    status: refreshed,
  });
  assert.equal(succeeded.dictation.templateStatusLoaded, true);
  assert.strictEqual(succeeded.dictation.templateStatus, refreshed);

  const retried = fixedIntakeBatchWorkflowReducer(succeeded, {
    type: "DICTATION_STATUS_LOAD_STARTED",
  });
  const failed = fixedIntakeBatchWorkflowReducer(retried, {
    type: "DICTATION_STATUS_LOAD_FAILED",
  });
  assert.equal(failed.dictation.templateStatusLoaded, true);
  assert.strictEqual(failed.dictation.templateStatus, refreshed);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(failed, {
    type: "DICTATION_STATUS_LOAD_FAILED",
  }), failed);
});

test("dictation template analysis and confirmation preserve the legacy run", () => {
  const oldRun = dictationRun(801);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    dictation: {
      ...createInitialFixedIntakeBatchWorkflowState().dictation,
      templateRun: oldRun,
    },
  };

  const analysisStarted = fixedIntakeBatchWorkflowReducer(initial, {
    type: "DICTATION_TEMPLATE_ANALYSIS_STARTED",
  });
  assert.equal(analysisStarted.dictation.templateBusy, true);
  assert.strictEqual(analysisStarted.dictation.templateRun, oldRun);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(analysisStarted, {
    type: "DICTATION_TEMPLATE_ANALYSIS_STARTED",
  }), analysisStarted);

  const returnedRun = dictationRun(802);
  const analysisSucceeded = fixedIntakeBatchWorkflowReducer(analysisStarted, {
    type: "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED",
    run: returnedRun,
  });
  assert.strictEqual(analysisSucceeded.dictation.templateRun, returnedRun);
  assert.equal(analysisSucceeded.dictation.templateBusy, true);

  const confirmationStarted = fixedIntakeBatchWorkflowReducer(analysisSucceeded, {
    type: "DICTATION_TEMPLATE_CONFIRMATION_STARTED",
  });
  assert.equal(confirmationStarted.dictation.templateBusy, true);
  assert.strictEqual(confirmationStarted.dictation.templateRun, returnedRun);

  const confirmedStatus = dictationStatus(true);
  const confirmed = fixedIntakeBatchWorkflowReducer(confirmationStarted, {
    type: "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED",
    status: confirmedStatus,
  });
  assert.strictEqual(confirmed.dictation.templateStatus, confirmedStatus);
  assert.strictEqual(confirmed.dictation.templateRun, returnedRun);
  assert.equal(confirmed.dictation.templateBusy, true);

  const finished = fixedIntakeBatchWorkflowReducer(confirmed, {
    type: "DICTATION_TEMPLATE_OPERATION_FINISHED",
  });
  assert.equal(finished.dictation.templateBusy, false);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "DICTATION_TEMPLATE_OPERATION_FINISHED",
  }), finished);
});

test("dictation page lifecycle keeps sequential partial success semantics", () => {
  const oldResult = dictationPageResult(301);
  const initial = {
    ...createInitialFixedIntakeBatchWorkflowState(),
    dictation: {
      ...createInitialFixedIntakeBatchWorkflowState().dictation,
      pageResults: { 301: oldResult },
      pageFailures: { 302: "旧失败", 303: "保留失败" },
      processingPageIds: [301],
    },
  };

  const started = fixedIntakeBatchWorkflowReducer(initial, {
    type: "DICTATION_PAGES_STARTED",
    pageIds: [301, 302, 302],
  });
  assert.deepEqual(started.dictation.processingPageIds, [301, 302]);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(started, {
    type: "DICTATION_PAGES_STARTED",
    pageIds: [301, 302],
  }), started);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(started, {
    type: "DICTATION_PAGES_STARTED",
    pageIds: [],
  }), started);

  const newResult = dictationPageResult(302);
  const succeeded = fixedIntakeBatchWorkflowReducer(started, {
    type: "DICTATION_PAGE_SUCCEEDED",
    pageId: 302,
    result: newResult,
  });
  assert.strictEqual(succeeded.dictation.pageResults[301], oldResult);
  assert.strictEqual(succeeded.dictation.pageResults[302], newResult);
  assert.equal(succeeded.dictation.pageFailures[302], undefined);
  assert.equal(succeeded.dictation.pageFailures[303], "保留失败");
  assert.equal(started.dictation.pageFailures[302], "旧失败");

  const failed = fixedIntakeBatchWorkflowReducer(succeeded, {
    type: "DICTATION_PAGE_FAILED",
    pageId: 302,
    error: "新失败",
  });
  assert.strictEqual(failed.dictation.pageResults[302], newResult);
  assert.equal(failed.dictation.pageFailures[302], "新失败");

  const finished = fixedIntakeBatchWorkflowReducer(failed, {
    type: "DICTATION_PAGE_FINISHED",
    pageId: 302,
  });
  assert.deepEqual(finished.dictation.processingPageIds, [301]);
  assert.strictEqual(fixedIntakeBatchWorkflowReducer(finished, {
    type: "DICTATION_PAGE_FINISHED",
    pageId: 999,
  }), finished);
});

test("dictation actions do not mutate the other workflow slices", () => {
  const initial = createInitialFixedIntakeBatchWorkflowState();
  const status = dictationStatus(true);
  const next = fixedIntakeBatchWorkflowReducer(initial, {
    type: "DICTATION_STATUS_LOAD_SUCCEEDED",
    status,
  });

  assert.strictEqual(next.batch, initial.batch);
  assert.strictEqual(next.grouping, initial.grouping);
  assert.strictEqual(next.quality, initial.quality);
  assert.strictEqual(next.ordinary, initial.ordinary);
  assert.strictEqual(next.answerSheet, initial.answerSheet);
  assert.strictEqual(next.dictation.templateStatus, status);
});

function populatedFixedIntakeState() {
  let state = createInitialFixedIntakeState({ classId: 1, assessmentVersionId: 12 });
  state = fixedIntakeReducer(state, { type: "DRAFT_EXPECTED_PAGES_CHANGED", expectedPages: "2" });
  state = fixedIntakeReducer(state, { type: "ANSWER_SOURCE_STARTED" });
  state = fixedIntakeReducer(state, {
    type: "ANSWER_SOURCE_SUCCEEDED",
    analysis: analysis(review([reviewItem(101, [0], [0])])),
  });
  state = fixedIntakeReducer(state, { type: "PREPARE_STARTED", requestKey: "prepare-key" });
  state = fixedIntakeReducer(state, { type: "PREPARE_SUCCEEDED", result: fixedResult() });
  state = fixedIntakeReducer(state, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence: groupedEvidence(),
  });
  state = fixedIntakeReducer(state, { type: "QUALITY_REJECTION_TOGGLED", pageId: 301 });
  state = fixedIntakeReducer(state, {
    type: "ORDINARY_ANALYSIS_SUCCEEDED",
    pageId: 301,
    run: ordinaryRun(301),
  });
  state = fixedIntakeReducer(state, {
    type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED",
    status: answerSheetStatus(true),
  });
  state = fixedIntakeReducer(state, {
    type: "ANSWER_SHEET_PAGE_SUCCEEDED",
    pageId: 301,
    result: answerSheetPageResult(301),
  });
  state = fixedIntakeReducer(state, {
    type: "DICTATION_STATUS_LOAD_SUCCEEDED",
    status: dictationStatus(true),
  });
  state = fixedIntakeReducer(state, {
    type: "DICTATION_PAGE_SUCCEEDED",
    pageId: 301,
    result: dictationPageResult(301),
  });
  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED",
    activeOperations: ["prepare:prepare-key", "grouping-evidence:101"],
  });
  return state;
}

function assertProcessingCleared(state: ReturnType<typeof createInitialFixedIntakeState>) {
  assert.deepEqual(state.answerSource, createInitialAnswerSourceState());
  assert.deepEqual(state.batch, createInitialFixedIntakeBatchWorkflowState().batch);
  assert.deepEqual(state.grouping, createInitialFixedIntakeBatchWorkflowState().grouping);
  assert.deepEqual(state.quality, createInitialFixedIntakeBatchWorkflowState().quality);
  assert.deepEqual(state.ordinary, createInitialFixedIntakeBatchWorkflowState().ordinary);
  assert.deepEqual(state.answerSheet, createInitialFixedIntakeBatchWorkflowState().answerSheet);
  assert.deepEqual(state.dictation, createInitialFixedIntakeBatchWorkflowState().dictation);
}

test("root state composes both existing domains and owns a fresh control slice", () => {
  const first = createInitialFixedIntakeState({ classId: 1, assessmentVersionId: 12 });
  const second = createInitialFixedIntakeState();

  assert.deepEqual(first.answerSource, createInitialAnswerSourceState());
  assert.deepEqual(first.context, { classId: 1, assessmentVersionId: 12 });
  assert.deepEqual(first.control, {
    scopeRevision: 0,
    draftRevision: 0,
    activeOperations: [],
  });
  assert.notStrictEqual(first.control.activeOperations, second.control.activeOperations);
});

test("root reducer routes legacy answer and workflow actions without cross-domain writes", () => {
  const initial = createInitialFixedIntakeState();
  const answerStarted = fixedIntakeReducer(initial, { type: "ANSWER_SOURCE_STARTED" });

  assert.equal(answerStarted.answerSource.busy, true);
  assert.strictEqual(answerStarted.batch, initial.batch);
  assert.strictEqual(answerStarted.control, initial.control);

  const prepareStarted = fixedIntakeReducer(answerStarted, {
    type: "PREPARE_STARTED",
    requestKey: "prepare-key",
  });
  assert.equal(prepareStarted.batch.busy, true);
  assert.equal(prepareStarted.batch.requestKey, "prepare-key");
  assert.strictEqual(prepareStarted.answerSource, answerStarted.answerSource);
  assert.strictEqual(prepareStarted.control, initial.control);
});

test("context invalidation increments scope once, preserves draft, and atomically clears processing", () => {
  const previous = populatedFixedIntakeState();
  const same = fixedIntakeReducer(previous, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: { type: "class_selected", classId: 1 },
  });
  assert.strictEqual(same, previous);

  const next = fixedIntakeReducer(previous, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: { type: "assessment_selected", assessmentVersionId: 13 },
  });

  assert.deepEqual(next.context, { classId: 1, assessmentVersionId: 13 });
  assert.strictEqual(next.draft, previous.draft);
  assert.equal(next.control.scopeRevision, 1);
  assert.equal(next.control.draftRevision, 0);
  assert.strictEqual(next.control.activeOperations, previous.control.activeOperations);
  assertProcessingCleared(next);
});

test("draft invalidation updates one logical input, increments both revisions, and keeps answer inputs exclusive", () => {
  const previous = populatedFixedIntakeState();
  const selected = fixedIntakeReducer(previous, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: {
      type: "student_paths_selected",
      studentPaths: ["/tmp/NEW_0001.jpg", "/tmp/NEW_0002.jpg"],
    },
  });
  assert.deepEqual(selected.draft.studentPaths, ["/tmp/NEW_0001.jpg", "/tmp/NEW_0002.jpg"]);
  assert.equal(selected.draft.pageCycle, null);
  assert.equal(selected.draft.expectedPages, "2");
  assert.equal(selected.control.scopeRevision, 1);
  assert.equal(selected.control.draftRevision, 1);
  assertProcessingCleared(selected);

  const fileSelected = fixedIntakeReducer(selected, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: { type: "answer_file_selected", answerPath: "/tmp/答案.docx" },
  });
  assert.equal(fileSelected.draft.answerPath, "/tmp/答案.docx");
  assert.equal(fileSelected.draft.answerText, "");
  assert.equal(fileSelected.control.scopeRevision, 2);
  assert.equal(fileSelected.control.draftRevision, 2);

  const textChanged = fixedIntakeReducer(fileSelected, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: { type: "answer_text_changed", answerText: "第1题 B" },
  });
  assert.equal(textChanged.draft.answerPath, null);
  assert.equal(textChanged.draft.answerText, "第1题 B");
  assert.equal(textChanged.control.scopeRevision, 3);
  assert.equal(textChanged.control.draftRevision, 3);

  const cleared = fixedIntakeReducer(textChanged, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: { type: "answer_cleared" },
  });
  assert.equal(cleared.draft.answerPath, null);
  assert.equal(cleared.draft.answerText, "");
  assert.equal(cleared.control.scopeRevision, 4);
  assert.equal(cleared.control.draftRevision, 4);
  assert.strictEqual(fixedIntakeReducer(cleared, {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: { type: "answer_cleared" },
  }), cleared);
});

test("active operation projection copies keys without changing revisions or business state", () => {
  const initial = createInitialFixedIntakeState();
  const activeOperations = ["cycle:1", "prepare:key-1"];
  const next = fixedIntakeReducer(initial, {
    type: "FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED",
    activeOperations,
  });

  assert.deepEqual(next.control.activeOperations, activeOperations);
  assert.notStrictEqual(next.control.activeOperations, activeOperations);
  assert.equal(next.control.scopeRevision, 0);
  assert.equal(next.control.draftRevision, 0);
  assert.strictEqual(next.answerSource, initial.answerSource);
  assert.strictEqual(next.batch, initial.batch);
  assert.strictEqual(fixedIntakeReducer(next, {
    type: "FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED",
    activeOperations: [...activeOperations],
  }), next);
});

test("guarded completion ignores stale success and finish but applies the current completion", () => {
  const initial = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_STARTED",
    requestKey: "prepare-key",
  });
  const prepared = fixedResult();

  const staleSuccess = fixedIntakeReducer(initial, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 1, draftRevision: 0 },
    completion: { type: "PREPARE_SUCCEEDED", result: prepared },
  });
  assert.strictEqual(staleSuccess, initial);

  const staleFinished = fixedIntakeReducer(initial, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 1 },
    completion: { type: "PREPARE_FINISHED" },
  });
  assert.strictEqual(staleFinished, initial);
  assert.equal(staleFinished.batch.busy, true);

  const currentSuccess = fixedIntakeReducer(initial, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0 },
    completion: { type: "PREPARE_SUCCEEDED", result: prepared },
  });
  assert.strictEqual(currentSuccess.batch.result, prepared);
  assert.equal(currentSuccess.batch.busy, true);

  const currentFinished = fixedIntakeReducer(currentSuccess, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: prepared.batchId },
    completion: { type: "PREPARE_FINISHED" },
  });
  assert.equal(currentFinished.batch.busy, false);
  assert.strictEqual(currentFinished.batch.result, prepared);
});

test("guarded material and grouping completions require the current batch", () => {
  let state = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult(),
  });
  state = fixedIntakeReducer(state, { type: "MATERIAL_CONFIRMATION_STARTED" });
  const materialConfirmation = {
    materialType: "ordinary_paper" as const,
    materialTypeDecision: "teacher_confirmed" as const,
    materialTypeConfidence: 1,
    groupingRoute: "preview_ready" as const,
    studentGroupCount: 3,
    groupingIssueCodes: [],
    nextAction: "确认学生顺序",
  };

  const staleMaterial = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: {
      type: "MATERIAL_CONFIRMATION_SUCCEEDED",
      confirmation: materialConfirmation,
    },
  });
  assert.strictEqual(staleMaterial, state);
  const staleMaterialFinished = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: { type: "MATERIAL_CONFIRMATION_FINISHED" },
  });
  assert.strictEqual(staleMaterialFinished, state);
  assert.equal(staleMaterialFinished.batch.confirmingType, true);

  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: {
      type: "MATERIAL_CONFIRMATION_SUCCEEDED",
      confirmation: materialConfirmation,
    },
  });
  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: { type: "MATERIAL_CONFIRMATION_FINISHED" },
  });
  assert.equal(state.batch.result?.materialType, "ordinary_paper");
  assert.equal(state.batch.confirmingType, false);

  state = fixedIntakeReducer(state, { type: "GROUPING_CONFIRMATION_STARTED" });
  const groupingConfirmation = {
    groupingRoute: "preview_ready" as const,
    studentGroupCount: 3,
    groupingIssueCodes: [],
    groupingConfirmed: true,
    groupingFirstStudentNo: "1",
    groupingLastStudentNo: "3",
    nextAction: "检查照片质量",
  };
  const staleGrouping = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: {
      type: "GROUPING_CONFIRMATION_SUCCEEDED",
      confirmation: groupingConfirmation,
    },
  });
  assert.strictEqual(staleGrouping, state);
  const staleGroupingFinished = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: { type: "GROUPING_CONFIRMATION_FINISHED" },
  });
  assert.strictEqual(staleGroupingFinished, state);
  assert.equal(staleGroupingFinished.grouping.confirming, true);

  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: {
      type: "GROUPING_CONFIRMATION_SUCCEEDED",
      confirmation: groupingConfirmation,
    },
  });
  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: { type: "GROUPING_CONFIRMATION_FINISHED" },
  });
  assert.equal(state.batch.result?.groupingConfirmed, true);
  assert.equal(state.grouping.confirming, false);
});

test("guarded quality and retake completions require the current batch and rejected page", () => {
  let qualityState = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult({
      materialType: "ordinary_paper",
      materialTypeDecision: "teacher_confirmed",
      groupingConfirmed: true,
    }),
  });
  qualityState = fixedIntakeReducer(qualityState, { type: "QUALITY_CONFIRMATION_STARTED" });
  const qualityConfirmation = {
    qualityReviewCompleted: true as const,
    mappedGroupCount: 2,
    rejectedGroupCount: 1,
    nextAction: "重拍异常页面",
  };
  const staleQuality = fixedIntakeReducer(qualityState, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: {
      type: "QUALITY_CONFIRMATION_SUCCEEDED",
      confirmation: qualityConfirmation,
    },
  });
  assert.strictEqual(staleQuality, qualityState);
  const currentQuality = fixedIntakeReducer(qualityState, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: {
      type: "QUALITY_CONFIRMATION_SUCCEEDED",
      confirmation: qualityConfirmation,
    },
  });
  assert.equal(currentQuality.batch.result?.qualityReviewCompleted, true);

  const rejectedEvidence = groupedEvidence(302).map((group) => ({
    ...group,
    pages: group.pages.map((page) => ({
      ...page,
      pageState: "rejected",
      qualityResult: "reject" as const,
      matchDecision: "rejected" as const,
    })),
  }));
  let retakeState = fixedIntakeReducer(currentQuality, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence: rejectedEvidence,
  });
  retakeState = fixedIntakeReducer(retakeState, { type: "RETAKE_STARTED", pageId: 302 });
  const replacement = {
    replacementPageId: 1302,
    activatedStudent: true,
    mappedGroupCount: 3,
    rejectedGroupCount: 0,
    nextAction: "进入批改终审",
  };

  const wrongRejectedPage = fixedIntakeReducer(retakeState, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      rejectedPageId: 999,
    },
    completion: { type: "RETAKE_SUCCEEDED", replacement },
  });
  assert.strictEqual(wrongRejectedPage, retakeState);

  const currentRejectedPage = fixedIntakeReducer(retakeState, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      rejectedPageId: 302,
    },
    completion: { type: "RETAKE_SUCCEEDED", replacement },
  });
  assert.equal(currentRejectedPage.batch.result?.rejectedGroupCount, 0);

  const refreshed = fixedIntakeReducer(currentRejectedPage, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence: groupedEvidence(1302).map((group) => ({
      ...group,
      pages: group.pages.map((page) => ({
        ...page,
        pageState: "mapped",
        qualityResult: "pass" as const,
        matchDecision: "teacher_confirmed" as const,
      })),
    })),
  });
  const disappearedRejectedPage = fixedIntakeReducer(refreshed, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      rejectedPageId: 302,
    },
    completion: { type: "RETAKE_SUCCEEDED", replacement },
  });
  assert.strictEqual(disappearedRejectedPage, refreshed);
});

test("guarded ordinary completions require the current eligible page and analysis run", () => {
  let state = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult({
      materialType: "ordinary_paper",
      materialTypeDecision: "teacher_confirmed",
      groupingConfirmed: true,
      qualityReviewCompleted: true,
    }),
  });
  const evidence = groupedEvidence(301).map((group) => ({
    ...group,
    pages: group.pages.map((page) => ({
      ...page,
      pageState: "mapped",
      qualityResult: "pass" as const,
      matchDecision: "teacher_confirmed" as const,
    })),
  }));
  state = fixedIntakeReducer(state, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence,
  });

  const stalePage = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      pageId: 999,
    },
    completion: {
      type: "ORDINARY_ANALYSIS_SUCCEEDED",
      pageId: 999,
      run: ordinaryRun(999),
    },
  });
  assert.strictEqual(stalePage, state);

  const currentRun = {
    ...ordinaryRun(301),
    ai_run_id: 9301,
  };
  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      pageId: 301,
    },
    completion: {
      type: "ORDINARY_ANALYSIS_SUCCEEDED",
      pageId: 301,
      run: currentRun,
    },
  });
  assert.strictEqual(state.ordinary.runs[301], currentRun);

  const confirming = fixedIntakeReducer(state, {
    type: "ORDINARY_CONFIRMATION_STARTED",
    pageIds: [301],
  });
  const confirmation = ordinaryConfirmation(301);
  const staleRun = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      pageId: 301,
      ordinaryRunId: 8301,
    },
    completion: {
      type: "ORDINARY_CONFIRMATION_SUCCEEDED",
      pageId: 301,
      confirmation,
    },
  });
  assert.strictEqual(staleRun, confirming);

  const staleFinished = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      pageId: 301,
      ordinaryRunId: 8301,
    },
    completion: { type: "ORDINARY_CONFIRMATION_FINISHED", pageId: 301 },
  });
  assert.strictEqual(staleFinished, confirming);
  assert.deepEqual(staleFinished.ordinary.confirmingPageIds, [301]);

  const currentConfirmation = {
    ...confirmation,
    confirmation: {
      ...confirmation.confirmation,
      ai_run_id: 9301,
    },
  };
  const current = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      batchId: 101,
      pageId: 301,
      ordinaryRunId: 9301,
    },
    completion: {
      type: "ORDINARY_CONFIRMATION_SUCCEEDED",
      pageId: 301,
      confirmation: currentConfirmation,
    },
  });
  assert.strictEqual(current.ordinary.confirmations[301], currentConfirmation);
});

test("guarded answer-sheet template completions require the current target page and analysis run", () => {
  let state = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult({
      materialType: "answer_sheet",
      materialTypeDecision: "teacher_confirmed",
      groupingConfirmed: true,
      qualityReviewCompleted: true,
    }),
  });
  const evidence = groupedEvidence(301).map((group) => ({
    ...group,
    pages: group.pages.map((page) => ({
      ...page,
      pageState: "mapped",
      qualityResult: "pass" as const,
      matchDecision: "teacher_confirmed" as const,
    })),
  }));
  state = fixedIntakeReducer(state, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence,
  });
  state = fixedIntakeReducer(state, {
    type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED",
    status: answerSheetStatus(false),
  });

  const run = answerSheetRun(9301);
  const stalePage = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "answer_sheet:101:301",
      pageId: 999,
    },
    completion: { type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED", run },
  });
  assert.strictEqual(stalePage, state);

  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "answer_sheet:101:301",
      pageId: 301,
    },
    completion: { type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED", run },
  });
  assert.strictEqual(state.answerSheet.templateRun, run);

  const confirming = fixedIntakeReducer(state, {
    type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_STARTED",
  });
  const refreshed = answerSheetStatus(true);
  const staleRun = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "answer_sheet:101:301",
      pageId: 301,
      answerSheetTemplateRunId: 8301,
    },
    completion: {
      type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED",
      status: refreshed,
    },
  });
  assert.strictEqual(staleRun, confirming);

  const current = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "answer_sheet:101:301",
      pageId: 301,
      answerSheetTemplateRunId: 9301,
    },
    completion: {
      type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED",
      status: refreshed,
    },
  });
  assert.strictEqual(current.answerSheet.templateStatus, refreshed);
  assert.equal(current.answerSheet.templateRun, null);
});

test("guarded dictation template completions require the current page and analysis run", () => {
  let state = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult({
      materialType: "dictation",
      materialTypeDecision: "teacher_confirmed",
      groupingConfirmed: true,
      qualityReviewCompleted: true,
    }),
  });
  const evidence = groupedEvidence(301).map((group) => ({
    ...group,
    pages: group.pages.map((page) => ({
      ...page,
      pageState: "mapped",
      qualityResult: "pass" as const,
      matchDecision: "teacher_confirmed" as const,
    })),
  }));
  state = fixedIntakeReducer(state, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence,
  });
  state = fixedIntakeReducer(state, {
    type: "DICTATION_STATUS_LOAD_SUCCEEDED",
    status: dictationStatus(false),
  });

  const run = dictationRun(9401);
  const stalePage = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "dictation:101:301",
      pageId: 999,
    },
    completion: { type: "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED", run },
  });
  assert.strictEqual(stalePage, state);

  state = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "dictation:101:301",
      pageId: 301,
    },
    completion: { type: "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED", run },
  });
  assert.strictEqual(state.dictation.templateRun, run);

  const confirming = fixedIntakeReducer(state, {
    type: "DICTATION_TEMPLATE_CONFIRMATION_STARTED",
  });
  const refreshed = dictationStatus(true);
  const staleRun = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "dictation:101:301",
      pageId: 301,
      dictationTemplateRunId: 8401,
    },
    completion: {
      type: "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED",
      status: refreshed,
    },
  });
  assert.strictEqual(staleRun, confirming);

  const current = fixedIntakeReducer(confirming, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      draftRevision: 0,
      scopeKey: "dictation:101:301",
      pageId: 301,
      dictationTemplateRunId: 9401,
    },
    completion: {
      type: "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED",
      status: refreshed,
    },
  });
  assert.strictEqual(current.dictation.templateStatus, refreshed);
  assert.strictEqual(current.dictation.templateRun, run);
});

test("guarded answer-source analysis completions require the current batch", () => {
  const prepared = fixedResult({
    reasonCodes: ["ANSWER_SOURCE_STRUCTURE_PENDING"],
  });
  let state = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: prepared,
  });
  state = fixedIntakeReducer(state, { type: "ANSWER_SOURCE_STARTED" });
  const resultAnalysis = analysis(review([reviewItem(105, [0], [0])]));

  const stale = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: { type: "ANSWER_SOURCE_SUCCEEDED", analysis: resultAnalysis },
  });
  assert.strictEqual(stale, state);

  const staleFailure = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: { type: "ANSWER_SOURCE_FAILED", error: "旧批次失败" },
  });
  assert.strictEqual(staleFailure, state);
  const staleReason = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: {
      type: "ANSWER_SOURCE_REASON_REPLACED",
      reasonCode: "ANSWER_SOURCE_STRUCTURE_FAILED",
    },
  });
  assert.strictEqual(staleReason, state);
  const staleFinished = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 999 },
    completion: { type: "ANSWER_SOURCE_FINISHED" },
  });
  assert.strictEqual(staleFinished, state);
  assert.equal(staleFinished.answerSource.busy, true);

  const currentSuccess = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: { type: "ANSWER_SOURCE_SUCCEEDED", analysis: resultAnalysis },
  });
  assert.strictEqual(currentSuccess.answerSource.analysis, resultAnalysis);

  const currentReason = fixedIntakeReducer(currentSuccess, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: {
      type: "ANSWER_SOURCE_REASON_REPLACED",
      reasonCode: "ANSWER_SOURCE_CONFIRMATION_REQUIRED",
    },
  });
  assert.deepEqual(currentReason.batch.result?.reasonCodes, [
    "ANSWER_SOURCE_CONFIRMATION_REQUIRED",
  ]);

  const currentFinished = fixedIntakeReducer(currentReason, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0, batchId: 101 },
    completion: { type: "ANSWER_SOURCE_FINISHED" },
  });
  assert.equal(currentFinished.answerSource.busy, false);
});

test("guarded answer resolution requires the current source run", () => {
  const oldReview = review([reviewItem(105, [0], [0])]);
  let state = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult({
      reasonCodes: ["ANSWER_SOURCE_CONFLICT_OR_MISSING"],
    }),
  });
  state = fixedIntakeReducer(state, {
    type: "ANSWER_SOURCE_SUCCEEDED",
    analysis: analysis(oldReview),
  });
  state = fixedIntakeReducer(state, { type: "ANSWER_SOURCE_RESOLUTION_STARTED" });
  const confirmedReview = review(oldReview.items, {
    route: "confirmed",
    sourceState: "ready",
    resolution: "confirmed_matches",
  });

  const staleRun = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, batchId: 101, runId: 999 },
    completion: {
      type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED",
      review: confirmedReview,
    },
  });
  assert.strictEqual(staleRun, state);
  assert.equal(staleRun.answerSource.busy, true);
  const staleReason = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, batchId: 101, runId: 999 },
    completion: { type: "ANSWER_SOURCE_REASON_REPLACED", reasonCode: null },
  });
  assert.strictEqual(staleReason, state);

  const currentSuccess = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, batchId: 101, runId: 32 },
    completion: {
      type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED",
      review: confirmedReview,
    },
  });
  assert.strictEqual(currentSuccess.answerSource.analysis?.review, confirmedReview);

  const currentReason = fixedIntakeReducer(currentSuccess, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, batchId: 101, runId: 32 },
    completion: { type: "ANSWER_SOURCE_REASON_REPLACED", reasonCode: null },
  });
  assert.deepEqual(currentReason.batch.result?.reasonCodes, []);

  const staleFinished = fixedIntakeReducer(currentReason, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, batchId: 101, runId: 999 },
    completion: { type: "ANSWER_SOURCE_FINISHED" },
  });
  assert.strictEqual(staleFinished, currentReason);
  assert.equal(staleFinished.answerSource.busy, true);

  const currentFinished = fixedIntakeReducer(currentReason, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: { scopeRevision: 0, batchId: 101, runId: 32 },
    completion: { type: "ANSWER_SOURCE_FINISHED" },
  });
  assert.equal(currentFinished.answerSource.busy, false);
});

test("guarded read completion requires the current material scope and eligible page", () => {
  let state = createInitialFixedIntakeState({ classId: 1, assessmentVersionId: 12 });
  state = fixedIntakeReducer(state, {
    type: "PREPARE_SUCCEEDED",
    result: fixedResult({
      materialType: "answer_sheet",
      materialTypeDecision: "teacher_confirmed",
      materialTypeConfidence: 1,
      materialTypeNeedsConfirmation: false,
      groupingConfirmed: true,
      qualityReviewCompleted: true,
    }),
  });
  const evidence = groupedEvidence().map((group) => ({
    ...group,
    pages: group.pages.map((page) => ({
      ...page,
      qualityResult: "pass" as const,
      matchDecision: "teacher_confirmed" as const,
    })),
  }));
  state = fixedIntakeReducer(state, {
    type: "GROUPING_EVIDENCE_SUCCEEDED",
    evidence,
  });

  const status = answerSheetStatus(true);
  const staleScope = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      batchId: 101,
      scopeKey: "answer_sheet:101:999",
    },
    completion: { type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED", status },
  });
  assert.strictEqual(staleScope, state);

  const currentStatus = fixedIntakeReducer(state, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      batchId: 101,
      scopeKey: "answer_sheet:101:301",
    },
    completion: { type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED", status },
  });
  assert.strictEqual(currentStatus.answerSheet.templateStatus, status);

  const pageResult = answerSheetPageResult(301);
  const stalePage = fixedIntakeReducer(currentStatus, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      batchId: 101,
      scopeKey: "answer_sheet:101:301",
      pageId: 999,
    },
    completion: {
      type: "ANSWER_SHEET_PAGE_SUCCEEDED",
      pageId: 999,
      result: pageResult,
    },
  });
  assert.strictEqual(stalePage, currentStatus);

  const currentPage = fixedIntakeReducer(currentStatus, {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED",
    identity: {
      scopeRevision: 0,
      batchId: 101,
      scopeKey: "answer_sheet:101:301",
      pageId: 301,
    },
    completion: {
      type: "ANSWER_SHEET_PAGE_SUCCEEDED",
      pageId: 301,
      result: pageResult,
    },
  });
  assert.strictEqual(currentPage.answerSheet.pageResults[301], pageResult);
});

test("guarded page-cycle completion writes suggestion and inferred pages only for the current draft", () => {
  const initial = fixedIntakeReducer(createInitialFixedIntakeState(), {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED",
    mutation: {
      type: "student_paths_selected",
      studentPaths: ["/tmp/NEW_0001.jpg", "/tmp/NEW_0002.jpg"],
    },
  });
  const pageCycle: PageCycleSuggestion = {
    expectedPagesPerAttempt: 3,
    confidence: 0.96,
    source: "visual_repeating_layout_v1",
    issueCodes: [],
    needsTeacherInput: false,
  };

  const stale = fixedIntakeReducer(initial, {
    type: "FIXED_INTAKE_PAGE_CYCLE_RECEIVED",
    identity: { scopeRevision: 0, draftRevision: 0 },
    pageCycle,
  });
  assert.strictEqual(stale, initial);

  const current = fixedIntakeReducer(initial, {
    type: "FIXED_INTAKE_PAGE_CYCLE_RECEIVED",
    identity: { scopeRevision: 1, draftRevision: 1 },
    pageCycle,
  });
  assert.strictEqual(current.draft.pageCycle, pageCycle);
  assert.equal(current.draft.expectedPages, "3");
  assert.deepEqual(current.control, initial.control);
  assert.strictEqual(current.batch, initial.batch);
  assert.strictEqual(current.answerSource, initial.answerSource);
});
