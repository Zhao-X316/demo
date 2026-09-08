from ui_navigation import expand_new_intake
from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""固定上传共享壳浏览器特征测试。

覆盖文件顺序/页数、可选答案资料、材料类型、学号与缺交归组、
联系表质量确认、重拍替换、失败重试和三材料终审路由。
三材料内部识别准确性及事务边界继续由各自专项测试与 Rust 测试负责。
"""

import re

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__fixedCalls = [];
window.__dialogCount = 0;
window.__fixedOptionsCalls = 0;
window.__prepareAttempts = 0;
window.__answerAnalyzeAttempts = 0;
window.__materialAttempts = 0;
window.__groupingAttempts = 0;
window.__qualityAttempts = 0;
window.__answerSheetProcessAttempts = 0;
window.__dictationProcessAttempts = 0;
window.__qualityConfirmed = false;
window.__rejectedPageIds = [];
window.__replaced = false;

const params = () => new URLSearchParams(window.location.search);
const scenario = () => params().get("scenario") || "main";
const material = () => params().get("material") || "unknown";
const isBatchResetScenario = () => scenario().includes("batch_reset");

const roster = [
  { studentId: 21, studentNo: "1", studentName: "张同学" },
  { studentId: 22, studentNo: "2", studentName: "李同学" },
  { studentId: 23, studentNo: "3", studentName: "王同学" },
  { studentId: 24, studentNo: "4", studentName: "赵同学" },
  { studentId: 25, studentNo: "5", studentName: "钱同学" },
];

const baseReview = (route, sourceState = "ready") => ({
  ingestBatchId: 101,
  sourceAiRunId: 901,
  sourceState,
  route,
  matchedCount: route === "ready_to_confirm" || route === "confirmed" ? 1 : 0,
  conflictCount: route === "blocked" || route === "kept_bound" || route === "adopted_new_version" ? 1 : 0,
  missingCount: 0,
  resolution: route === "confirmed" ? "confirmed_matches" : route === "kept_bound" ? "kept_bound" : null,
  adoption: null,
  items: route === "blocked" || route === "kept_bound" || route === "adopted_new_version" ? [{
    assessmentItemId: 502,
    orderIndex: 1,
    questionNo: "2",
    questionType: "short_answer",
    questionStem: "分析洋务运动失败原因",
    boundAnswerKeyVersionId: 601,
    boundAnswerJson: JSON.stringify({ reference_answer: "制度与管理原因" }),
    boundRubricVersionId: 602,
    boundLinkSetId: 603,
    boundRubricPoints: [
      {
        stableId: "old-point-1",
        orderIndex: 0,
        canonicalText: "没有改变封建制度",
        maxScore: 2,
        confirmedKnowledgeTitles: ["洋务运动局限"],
        confirmedAbilityTitles: ["因果分析"],
      },
      {
        stableId: "old-point-2",
        orderIndex: 1,
        canonicalText: "内部管理腐败",
        maxScore: 2,
        confirmedKnowledgeTitles: ["洋务企业管理"],
        confirmedAbilityTitles: [],
      },
    ],
    candidateId: 701,
    candidateAnswerJson: JSON.stringify({
      reference_answer: "制度、管理两点",
      rubric_points: [
        { stable_id: "candidate-1", order_index: 0, canonical_text: "没有改变封建制度", max_score: 2 },
        { stable_id: "candidate-2", order_index: 1, canonical_text: "管理腐败严重", max_score: 2 },
      ],
    }),
    sourceAnchorJson: JSON.stringify({ page_no: 1 }),
    matchState: "conflict",
  }] : [{
    assessmentItemId: 501,
    orderIndex: 0,
    questionNo: "1",
    questionType: "single",
    questionStem: "洋务运动后期口号",
    boundAnswerKeyVersionId: 601,
    boundAnswerJson: JSON.stringify({ correct_labels: ["B"] }),
    boundRubricVersionId: 602,
    boundLinkSetId: 603,
    boundRubricPoints: [],
    candidateId: 700,
    candidateAnswerJson: JSON.stringify({ correct_labels: ["B"] }),
    sourceAnchorJson: JSON.stringify({ page_no: 1 }),
    matchState: "matched",
  }],
});

const preparedResult = (request) => {
  const directMaterial = material();
  const direct = directMaterial !== "unknown";
  const batchId = isBatchResetScenario() && window.__prepareAttempts > 1 ? 102 : 101;
  return {
    batchId,
    batchPublicId: `batch-${batchId}`,
    documents: request.studentPaths.map((path) => ({
      role: "student_work",
      format: "jpeg",
      originalName: path.split("/").pop(),
      pageCount: 1,
    })),
    studentDocumentCount: request.studentPaths.length,
    studentPageCount: request.studentPaths.length,
    answerDocumentCount: request.answerPath || request.answerText ? 1 : 0,
    route: "ready_for_batch_confirm",
    targetCount: direct ? 1 : 3,
    readyCount: direct ? 1 : 0,
    reviewCount: direct ? 0 : 3,
    blockedCount: 0,
    completedCount: 0,
    reasonCodes: direct ? [] : [
      "MATERIAL_TYPE_CONFIRMATION_REQUIRED",
      "STUDENT_RANGE_OR_ABSENCE_CONFIRMATION_REQUIRED",
      ...(request.answerPath || request.answerText ? ["ANSWER_SOURCE_STRUCTURE_PENDING"] : []),
    ],
    orderPolicy: "natural_filename_then_capture_time",
    orderConfidence: 0.97,
    orderConflictCodes: [],
    materialType: direct ? directMaterial : "unknown",
    materialTypeDecision: direct ? "teacher_confirmed" : "suggested",
    materialTypeConfidence: direct ? 1 : 0.55,
    materialTypeNeedsConfirmation: !direct,
    groupingRoute: "preview_ready",
    studentGroupCount: direct ? 1 : 3,
    groupingIssueCodes: [],
    expectedPagesPerAttempt: request.expectedPagesPerAttempt,
    pageCycleSource: "visual_repeating_layout_v1",
    pageCycleConfidence: 0.96,
    pageCycleNeedsTeacherInput: false,
    groupingRoster: roster,
    groupingConfirmed: direct,
    groupingFirstStudentNo: direct ? "1" : null,
    groupingLastStudentNo: direct ? "1" : null,
    qualityReviewCompleted: direct,
    mappedGroupCount: direct ? 1 : 0,
    rejectedGroupCount: 0,
    nextAction: direct ? "进入批改终审" : "确认材料类型",
  };
};

const groupingEvidence = () => {
  if (material() !== "unknown") {
    const secondBatch = isBatchResetScenario() && window.__prepareAttempts > 1;
    const pageId = secondBatch ? 401 : 301;
    const originalName = secondBatch ? "NEW_0001.jpg" : "IMG_0001.jpg";
    return [{
      groupIndex: 0,
      studentId: 21,
      studentNo: "1",
      studentName: "张同学",
      pages: [{
        pageId,
        replacedPageId: null,
        pageNo: 1,
        importIndex: 0,
        archivedPath: `/tmp/${originalName}`,
        originalName,
        pageState: "mapped",
        qualityResult: "pass",
        matchDecision: "teacher_confirmed",
      }],
    }];
  }
  const students = [roster[1], roster[3], roster[4]];
  return students.map((student, groupIndex) => ({
    groupIndex,
    studentId: student.studentId,
    studentNo: student.studentNo,
    studentName: student.studentName,
    pages: [0, 1].map((pageOffset) => {
      const originalPageId = 301 + groupIndex * 2 + pageOffset;
      const replaced = window.__replaced && originalPageId === 302;
      const rejected = window.__qualityConfirmed
        && window.__rejectedPageIds.includes(originalPageId)
        && !replaced;
      return {
        pageId: replaced ? 1302 : originalPageId,
        replacedPageId: replaced ? 302 : null,
        pageNo: pageOffset + 1,
        importIndex: groupIndex * 2 + pageOffset,
        archivedPath: replaced ? "/tmp/RETAKE_0002.jpg" : `/tmp/IMG_000${groupIndex * 2 + pageOffset + 1}.jpg`,
        originalName: replaced ? "RETAKE_0002.jpg" : `IMG_000${groupIndex * 2 + pageOffset + 1}.jpg`,
        pageState: window.__qualityConfirmed ? (rejected ? "rejected" : "mapped") : "prepared",
        qualityResult: window.__qualityConfirmed ? (rejected ? "reject" : "pass") : null,
        matchDecision: window.__qualityConfirmed ? (rejected ? "rejected" : "teacher_confirmed") : "suggested",
      };
    }),
  }));
};

window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__fixedCalls.push({ cmd, args });
    if (cmd === "plugin:dialog|open") {
      window.__dialogCount += 1;
      if (scenario() === "unsupported" && window.__dialogCount === 1) return ["/tmp/IMG_0001.png"];
      if (scenario() === "answer_file" && window.__dialogCount === 2) return "/tmp/洋务运动答案.docx";
      if (isBatchResetScenario() && window.__dialogCount === 2) {
        return [
          "/tmp/NEW_0001.jpg",
          "/tmp/NEW_0002.jpg",
          "/tmp/NEW_0003.jpg",
          "/tmp/NEW_0004.jpg",
          "/tmp/NEW_0005.jpg",
          "/tmp/NEW_0006.jpg",
        ];
      }
      if (window.__dialogCount > 1) return "/tmp/RETAKE_0002.jpg";
      return [
        "/tmp/IMG_0001.jpg",
        "/tmp/IMG_0002.jpg",
        "/tmp/IMG_0003.jpg",
        "/tmp/IMG_0004.jpg",
        "/tmp/IMG_0005.jpg",
        "/tmp/IMG_0006.jpg",
      ];
    }
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "list_profile_scope_options") return [];
    if (cmd === "class_operations_dashboard") {
      return {
        meta: {
          schema_version: 2,
          rule_version: "m6.1-operations-v2",
          calculated_at: "2026-08-02T12:00:00Z",
          as_of_date: args.asOfDate,
          recitation_watermark: null,
          exam_watermark: null,
        },
        class: {
          id: 1,
          name: "八年级一班",
          term: "2026秋",
          textbook: "中国历史八上",
          enabled_student_count: 5,
        },
        recitation: {
          expected_student_count: 0,
          completed_student_count: 0,
          expected_task_count: 0,
          confirmed_task_count: 0,
          submitted_task_count: 0,
          not_submitted_student_count: 0,
          pending_teacher_review_count: 0,
          overdue_pending_review_count: 0,
          recognition_failure_count: 0,
          recognition_processing_count: 0,
          denominator_note: "暂无任务。",
        },
        exam: {
          active_assessment_count: 1,
          expected_submission_count: 5,
          submitted_submission_count: 0,
          missing_submission_count: 5,
          ingesting_attempt_count: 0,
          grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0,
          published_submission_count: 0,
          open_pipeline_issue_count: 0,
          denominator_note: "按当前作业计算。",
        },
        students: [],
        actions: [],
      };
    }
    if (cmd === "latest_class_profile") return null;
    if (cmd === "list_class_teaching_events"
        || cmd === "list_class_teaching_inputs"
        || cmd === "list_class_action_drafts") return [];
    if (cmd === "students_list"
        || cmd === "questions_list"
        || cmd === "kp_list"
        || cmd === "exam_answers_list") return [];
    if (cmd === "exam_objective_workbench"
        || cmd === "exam_answer_sheet_subjective_workbench"
        || cmd === "exam_dictation_workbench") return { rows: [], attempts: [] };
    if (cmd === "workspace_tasks") return [];
    if (cmd === "workspace_exam_review") return {
      task:{kind:args.kind,sourceId:args.sourceId,title:"当前测试批次",classId:1,className:"八年级一班",studentName:null,status:"needs_review",updatedAt:"2026-09-07",assessmentVersionId:12,attemptIds:[301,401],hasEvidence:false},
      objective:{rows:[],attempts:[]},subjective:{rows:[],attempts:[]},dictation:{rows:[],attempts:[]}
    };
    if (cmd === "exam_fixed_intake_options") {
      window.__fixedOptionsCalls += 1;
      if (scenario() === "empty") return [];
      if (scenario() === "options_matrix" || scenario() === "options_refresh") {
        return [
          {
            classId: 1,
            className: "八年级一班",
            assessmentId: 11,
            assessmentVersionId: 12,
            assessmentTitle: "洋务运动随堂练习",
            revision: 3,
            templateVersion: "fixed-v3",
            itemCount: 2,
            isDefault: true,
            defaultSelectionPublicId: "selection-12",
          },
          {
            classId: 1,
            className: "八年级一班",
            assessmentId: 13,
            assessmentVersionId: 13,
            assessmentTitle: "戊戌变法随堂练习",
            revision: 1,
            templateVersion: "fixed-v1",
            itemCount: 3,
            isDefault: false,
            defaultSelectionPublicId: "selection-13",
          },
          {
            classId: 2,
            className: "八年级二班",
            assessmentId: 22,
            assessmentVersionId: 22,
            assessmentTitle: "辛亥革命随堂练习",
            revision: 2,
            templateVersion: "fixed-v2",
            itemCount: 4,
            isDefault: true,
            defaultSelectionPublicId: "selection-22",
          },
        ];
      }
      return [{
        classId: 1,
        className: "八年级一班",
        assessmentId: 11,
        assessmentVersionId: 12,
        assessmentTitle: "洋务运动随堂练习",
        revision: 3,
        templateVersion: "fixed-v3",
        itemCount: 2,
        isDefault: true,
        defaultSelectionPublicId: "selection-12",
      }];
    }
    if (cmd === "exam_fixed_intake_infer_page_cycle") {
      return {
        expectedPagesPerAttempt: 2,
        confidence: 0.96,
        source: "visual_repeating_layout_v1",
        issueCodes: [],
        needsTeacherInput: false,
      };
    }
    if (cmd === "exam_fixed_intake_prepare") {
      window.__prepareAttempts += 1;
      if ((scenario() === "prepare_retry"
          || scenario() === "prepare_retry_pages"
          || scenario() === "prepare_retry_text")
          && window.__prepareAttempts === 1) {
        throw new Error("模拟批次准备失败");
      }
      return preparedResult(args.request);
    }
    if (cmd === "exam_answer_source_analyze") {
      window.__answerAnalyzeAttempts += 1;
      if (scenario() === "main" && window.__answerAnalyzeAttempts === 1) {
        throw new Error("模拟答案整理暂时失败");
      }
      const review = scenario() === "conflict" || scenario() === "keep"
        ? baseReview("blocked")
        : baseReview("ready_to_confirm");
      return {
        run: {
          ai_run_id: 901,
          status: "succeeded",
          output: { state: review.sourceState, confidence: 0.98, issue_codes: [] },
          failure: null,
        },
        review,
      };
    }
    if (cmd === "exam_answer_source_confirm_matches") return baseReview("confirmed");
    if (cmd === "exam_answer_source_keep_bound") return baseReview("kept_bound");
    if (cmd === "exam_answer_source_adopt_new_version") {
      const review = baseReview("adopted_new_version");
      review.adoption = {
        sourceAssessmentVersionId: 12,
        adoptedAssessmentVersionId: 13,
        adoptedAssessmentVersionPublicId: "assessment-version-13",
        adoptedAssessmentRevision: 4,
        changedItemCount: 1,
        changedRubricCount: 1,
        carriedKnowledgeLinkCount: 2,
        carriedAbilityLinkCount: 1,
        newRubricPointCount: 0,
        retiredRubricPointCount: 0,
        unlinkedNewRubricPointCount: 0,
        droppedKnowledgeLinkCount: 0,
        droppedAbilityLinkCount: 0,
        currentBatchUnchanged: true,
      };
      return review;
    }
    if (cmd === "exam_fixed_intake_confirm_material_type") {
      window.__materialAttempts += 1;
      if (scenario() === "main" && window.__materialAttempts === 1) {
        throw new Error("模拟材料类型确认失败");
      }
      return {
        materialType: args.materialType,
        materialTypeDecision: "teacher_confirmed",
        materialTypeConfidence: 1,
        groupingRoute: "preview_ready",
        studentGroupCount: 3,
        groupingIssueCodes: [],
        nextAction: "确认学生顺序",
      };
    }
    if (cmd === "exam_fixed_intake_confirm_grouping") {
      window.__groupingAttempts += 1;
      if (scenario() === "main" && window.__groupingAttempts === 1) {
        throw new Error("模拟学生归组确认失败");
      }
      return {
        groupingRoute: "preview_ready",
        studentGroupCount: 3,
        groupingIssueCodes: [],
        groupingConfirmed: true,
        groupingFirstStudentNo: args.firstStudentNo,
        groupingLastStudentNo: "5",
        nextAction: "确认照片清晰度",
      };
    }
    if (cmd === "exam_fixed_intake_grouping_evidence") return groupingEvidence();
    if (cmd === "exam_fixed_intake_confirm_grouping_quality") {
      window.__qualityAttempts += 1;
      if (scenario() === "main" && window.__qualityAttempts === 1) {
        throw new Error("模拟页面质量确认失败");
      }
      window.__qualityConfirmed = true;
      window.__rejectedPageIds = [...args.rejectedPageIds];
      return {
        qualityReviewCompleted: true,
        mappedGroupCount: args.rejectedPageIds.length ? 2 : 3,
        rejectedGroupCount: args.rejectedPageIds.length ? 1 : 0,
        nextAction: "进入对应材料识别",
      };
    }
    if (cmd === "exam_fixed_intake_replace_rejected_page") {
      window.__replaced = true;
      return {
        replacementPageId: 1302,
        activatedStudent: true,
        mappedGroupCount: 3,
        rejectedGroupCount: 0,
        nextAction: "进入批改终审",
      };
    }
    if (cmd === "exam_ordinary_paper_analyze_page") {
      if (scenario() === "ordinary_batch_reset" || scenario() === "options_refresh") {
        return {
          ai_run_id: 8000 + args.pageId,
          status: "succeeded",
          output: {
            schema_version: 1,
            page_id: args.pageId,
            expected_page_no: 1,
            state: "ready",
            quality: { result: "pass", issue_codes: [] },
            alignment: { confidence: 0.99 },
            regions: [{
              assessment_item_id: 501,
              region_index: 0,
              mapping_confidence: 0.99,
              mark_cells: [{ label: "A" }, { label: "B" }],
            }],
            printed_questions: [],
            confidence: 0.99,
            issue_codes: [],
          },
          failure: null,
        };
      }
      return {
        ai_run_id: 8000 + args.pageId,
        status: "failed",
        output: null,
        failure: { code: "CHARACTERIZATION_ONLY", safe_message: "等待普通卷专项处理", retryable: false },
      };
    }
    if (cmd === "exam_ordinary_paper_confirm_page_structure") {
      return {
        confirmation: {
          id: 6000 + args.pageId,
          ai_run_id: args.aiRunId,
          page_id: args.pageId,
          alignment_revision_id: 6100 + args.pageId,
          region_revision_ids: [7000 + args.pageId],
          confirmed_by: "local_teacher",
          created_at: "2026-08-02T12:10:00Z",
        },
        alignment: { id: 6100 + args.pageId, decision: "teacher_confirmed" },
        regions: [{
          id: 7000 + args.pageId,
          assessment_item_id: 501,
          region_index: 0,
          decision: "teacher_confirmed",
        }],
      };
    }
    if (cmd === "exam_ordinary_paper_sync_questions") {
      return {
        schema_version: 1,
        state: "completed",
        assessment_version_id: 12,
        page_no: 1,
        source_page_id: args.pageId,
        source_ai_run_id: args.aiRunId,
        printed_question_count: scenario() === "options_refresh" ? 1 : 0,
        eligible_count: scenario() === "options_refresh" ? 1 : 0,
        enqueued_count: 0,
        matched_count: scenario() === "options_refresh" ? 1 : 0,
        candidate_created_count: 0,
        needs_review_count: 0,
        privacy_rejected_count: 0,
        low_confidence_skipped_count: 0,
        failed_count: 0,
        reused_existing_source: scenario() === "options_refresh",
      };
    }
    if (cmd === "exam_objective_recognize_region") return { status: "queued" };
    if (cmd === "exam_answer_sheet_template_status") {
      return {
        assessmentVersionId: 12,
        pageNo: 1,
        activeTemplate: {
          id: 801,
          public_id: "answer-sheet-template-801",
          assessment_version_id: 12,
          revision: 2,
          template_version: "answer-sheet-v2",
          page_no: 1,
          blank_artifact_id: 802,
          source_ai_run_id: 803,
          confirmed_by: "local_teacher",
          state: "active",
          created_at: "2026-08-02T12:10:00Z",
        },
        templateSet: {
          assessment_version_id: 12,
          template_version: "answer-sheet-v2",
          ready: true,
          template_set_hash: "template-set-hash",
          pages: [{
            page_no: 1,
            expected_item_count: 2,
            objective_item_count: 1,
            subjective_item_count: params().get("subjective") === "1" ? 1 : 0,
            active_template_revision_id: 801,
            ready: true,
            issue_codes: [],
          }],
          issue_codes: [],
        },
      };
    }
    if (cmd === "exam_answer_sheet_process_page") {
      window.__answerSheetProcessAttempts += 1;
      if (scenario() === "answer_sheet_batch_reset" && window.__answerSheetProcessAttempts === 1) {
        throw new Error("模拟首批答题卡处理失败");
      }
      const hasSubjective = params().get("subjective") === "1";
      return {
        structure: {
          materialization: {
            id: 901,
            page_id: args.pageId,
            template_revision_id: 801,
            alignment_revision_id: 902,
            region_revision_ids: hasSubjective ? [903, 904] : [903],
          },
          regions: [{ id: 903, assessment_item_id: 501, region_index: 0, decision: "teacher_confirmed" }],
          routes: [{
            answer_region_revision_id: 903,
            assessment_item_id: 501,
            region_index: 0,
            recognition_route: "objective_omr",
            question_type: "single",
          }],
        },
        observations: [{
          observation: {
            id: 905,
            answer_region_revision_id: 903,
            result_state: "recognized",
            confidence: 0.99,
          },
          suggestion: {
            id: 906,
            outcome: "correct",
            batch_eligible: true,
            exclusion_reason: null,
          },
        }],
        subjectiveRegions: hasSubjective ? [{
          answerRegionRevisionId: 904,
          assessmentItemId: 502,
          regionIndex: 1,
          cropArtifactId: 907,
          state: "awaiting_handwriting_recognition",
          nextAction: "老师终审",
        }] : [],
        subjectiveTranscriptions: hasSubjective ? [{ result_state: "recognized" }] : [],
        subjectiveFailures: [],
      };
    }
    if (cmd === "exam_dictation_template_status") {
      return {
        assessmentVersionId: 12,
        pageNo: 1,
        activeTemplate: {
          id: 1001,
          assessment_version_id: 12,
          revision: 3,
          template_version: "dictation-v3",
          page_no: 1,
          source_ai_run_id: 1002,
          state: "active",
        },
      };
    }
    if (cmd === "exam_dictation_process_page") {
      window.__dictationProcessAttempts += 1;
      if (scenario() === "dictation_batch_reset" && window.__dictationProcessAttempts === 1) {
        return new Promise(() => {});
      }
      return {
        structure: {
          materialization: {
            id: 1101,
            page_id: args.pageId,
            template_revision_id: 1001,
            alignment_revision_id: 1102,
            region_revision_ids: [1103],
          },
          regions: [{ id: 1103, assessment_item_id: 502, region_index: 0, decision: "teacher_confirmed" }],
        },
        transcriptions: [{
          transcription: {
            id: 1104,
            answer_region_revision_id: 1103,
            revision: 1,
            result_state: "recognized",
            raw_ocr_text: "1840年",
            normalized_text: "1840 年",
            teacher_corrected_text: null,
            confidence: 0.99,
          },
          observation: { id: 1105, result: "exact", suggested_score: 2 },
        }],
      };
    }
    return [];
  },
};
"""


STUDENT_PATHS = [f"/tmp/IMG_000{index}.jpg" for index in range(1, 7)]
SECOND_STUDENT_PATHS = [f"/tmp/NEW_000{index}.jpg" for index in range(1, 7)]


def enter_exam(page) -> None:
    navigate_exam(page)
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()


def open_exam(page, url: str) -> None:
    page.goto(url)
    page.wait_for_load_state("networkidle")
    enter_exam(page)


def select_student_papers(page, first_file: str = "IMG_0001.jpg", fourth_file: str = "IMG_0004.jpg") -> None:
    expand_new_intake(page)
    page.get_by_role("button", name="选择试卷").click()
    expect(page.get_by_text("已选 6 份", exact=True)).to_be_visible()
    expect(page.get_by_text(first_file, exact=True)).to_be_visible()
    expect(page.get_by_text(fourth_file, exact=True)).to_be_visible()
    expect(page.get_by_text("另有 2 份", exact=True)).to_be_visible()
    expect(page.get_by_text("检测到版式每 2 页重复 · 可修改", exact=True)).to_be_visible()


def prepare_with_pasted_answer(page) -> None:
    select_student_papers(page)
    page.get_by_placeholder("也可以在这里粘贴答案").fill("  第1题 B；第2题制度局限  ")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()


def calls(page, command: str):
    return page.evaluate(
        "([command]) => window.__fixedCalls.filter((item) => item.cmd === command)",
        [command],
    )


def assert_no_authority_writes(page) -> None:
    forbidden = {
        "exam_objective_accept_suggestion",
        "exam_objective_teacher_correct",
        "exam_objective_batch_confirm",
        "exam_objective_publish_attempt",
        "exam_answer_sheet_subjective_accept_suggestion",
        "exam_answer_sheet_subjective_teacher_correct",
        "exam_answer_sheet_subjective_publish_attempt",
        "exam_dictation_accept_suggestion",
        "exam_dictation_teacher_score",
        "exam_dictation_publish_attempt",
        "grade_save",
        "question_add",
        "kp_add",
    }
    invoked = set(page.evaluate("window.__fixedCalls.map((item) => item.cmd)"))
    assert not forbidden.intersection(invoked), forbidden.intersection(invoked)


def test_empty_and_input_guards(browser, base_url: str) -> None:
    empty = browser.new_page(viewport={"width": 1440, "height": 1000})
    empty.add_init_script(MOCK_SCRIPT)
    open_exam(empty, f"{base_url}?scenario=empty")
    expect(empty.get_by_text("还没有可上传的固定卷作业", exact=True)).to_be_visible()
    expect(empty.get_by_text(re.compile("不会临时拼出不可追溯的题目或答案"))).to_be_visible()
    expect(empty.get_by_role("button", name="上传并开始整理")).to_have_count(0)

    unsupported = browser.new_page(viewport={"width": 1440, "height": 1000})
    unsupported.add_init_script(MOCK_SCRIPT)
    open_exam(unsupported, f"{base_url}?scenario=unsupported")
    unsupported.get_by_role("button", name="选择试卷").click()
    expect(unsupported.locator(".error")).to_contain_text("学生试卷只支持 JPG、JPEG 或 PDF")
    expect(unsupported.get_by_text("未选择", exact=True)).to_be_visible()
    assert not calls(unsupported, "exam_fixed_intake_infer_page_cycle")
    assert not calls(unsupported, "exam_fixed_intake_prepare")


def test_prepare_retry_keeps_idempotency(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=prepare_retry")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.locator(".error")).to_contain_text("模拟批次准备失败")
    expect(page.get_by_text("已选 6 份", exact=True)).to_be_visible()
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    prepare_calls = calls(page, "exam_fixed_intake_prepare")
    assert len(prepare_calls) == 2
    first = prepare_calls[0]["args"]["request"]
    second = prepare_calls[1]["args"]["request"]
    assert first["studentPaths"] == STUDENT_PATHS
    assert first["expectedPagesPerAttempt"] == 2
    assert first["materialType"] == "auto"
    assert first["idempotencyKey"] == second["idempotencyKey"]
    assert first["idempotencyKey"]
    assert_no_authority_writes(page)


def test_prepare_inputs_rotate_idempotency(browser, base_url: str) -> None:
    pages = browser.new_page(viewport={"width": 1440, "height": 1000})
    pages.add_init_script(MOCK_SCRIPT)
    open_exam(pages, f"{base_url}?scenario=prepare_retry_pages")
    select_student_papers(pages)
    pages.get_by_role("button", name="上传并开始整理").click()
    expect(pages.locator(".error")).to_contain_text("模拟批次准备失败")
    pages.locator("details.intake-advanced summary").click()
    pages.get_by_label("每名学生固定页数").fill("3")
    pages.get_by_role("button", name="上传并开始整理").click()
    expect(pages.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    page_requests = [item["args"]["request"] for item in calls(pages, "exam_fixed_intake_prepare")]
    assert len(page_requests) == 2
    assert page_requests[0]["expectedPagesPerAttempt"] == 2
    assert page_requests[1]["expectedPagesPerAttempt"] == 3
    assert page_requests[0]["idempotencyKey"] != page_requests[1]["idempotencyKey"]
    assert_no_authority_writes(pages)

    text = browser.new_page(viewport={"width": 1440, "height": 1000})
    text.add_init_script(MOCK_SCRIPT)
    open_exam(text, f"{base_url}?scenario=prepare_retry_text")
    select_student_papers(text)
    text.get_by_placeholder("也可以在这里粘贴答案").fill("  第1题 B  ")
    text.get_by_role("button", name="上传并开始整理").click()
    expect(text.locator(".error")).to_contain_text("模拟批次准备失败")
    text.get_by_placeholder("也可以在这里粘贴答案").fill("  第1题 B；第2题 制度局限  ")
    text.get_by_role("button", name="上传并开始整理").click()
    expect(text.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    text_requests = [item["args"]["request"] for item in calls(text, "exam_fixed_intake_prepare")]
    assert len(text_requests) == 2
    assert text_requests[0]["answerText"] == "第1题 B"
    assert text_requests[1]["answerText"] == "第1题 B；第2题 制度局限"
    assert text_requests[0]["idempotencyKey"] != text_requests[1]["idempotencyKey"]
    assert_no_authority_writes(text)


def test_context_switch_resets_session_but_keeps_draft(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=options_matrix")
    select_student_papers(page)
    page.locator("details.intake-advanced summary").click()
    page.get_by_label("每名学生固定页数").fill("3")
    page.get_by_placeholder("也可以在这里粘贴答案").fill("  班级共用答案  ")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()

    expand_new_intake(page)
    page.get_by_label("批改哪份作业").select_option("13")
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_have_count(0)
    expect(page.get_by_text("已选 6 份", exact=True)).to_be_visible()
    expect(page.get_by_label("每名学生固定页数")).to_have_value("3")
    expect(page.get_by_placeholder("也可以在这里粘贴答案")).to_have_value("  班级共用答案  ")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    assert calls(page, "exam_fixed_intake_prepare")[-1]["args"]["request"]["assessmentVersionId"] == 13

    expand_new_intake(page)
    page.get_by_label("班级").select_option("2")
    expect(page.get_by_label("批改哪份作业")).to_have_value("22")
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_have_count(0)
    expect(page.get_by_text("已选 6 份", exact=True)).to_be_visible()
    expect(page.get_by_label("每名学生固定页数")).to_have_value("3")
    expect(page.get_by_placeholder("也可以在这里粘贴答案")).to_have_value("  班级共用答案  ")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    assert calls(page, "exam_fixed_intake_prepare")[-1]["args"]["request"]["assessmentVersionId"] == 22
    assert_no_authority_writes(page)


def test_answer_file_request(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=answer_file")
    select_student_papers(page)
    page.get_by_role("button", name="选择答案").click()
    expect(page.get_by_text("洋务运动答案.docx", exact=True)).to_be_visible()
    expect(page.get_by_placeholder("已选择答案文件")).to_be_disabled()
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    request = calls(page, "exam_fixed_intake_prepare")[0]["args"]["request"]
    assert request["answerPath"] == "/tmp/洋务运动答案.docx"
    assert request["answerText"] is None
    expand_new_intake(page)
    page.get_by_role("button", name="清除答案资料").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_have_count(0)
    expect(page.get_by_text("已选 6 份", exact=True)).to_be_visible()
    expect(page.get_by_placeholder("也可以在这里粘贴答案")).to_be_enabled()
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    second_request = calls(page, "exam_fixed_intake_prepare")[1]["args"]["request"]
    assert second_request["answerPath"] is None
    assert second_request["answerText"] is None
    assert_no_authority_writes(page)


def test_shared_shell_and_failure_retention(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1200})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=main")
    prepare_with_pasted_answer(page)

    infer_call = calls(page, "exam_fixed_intake_infer_page_cycle")[0]
    assert infer_call["args"] == {"studentPaths": STUDENT_PATHS}
    request = calls(page, "exam_fixed_intake_prepare")[0]["args"]["request"]
    assert request["assessmentVersionId"] == 12
    assert request["studentPaths"] == STUDENT_PATHS
    assert request["answerPath"] is None
    assert request["answerText"] == "第1题 B；第2题制度局限"
    assert request["expectedPagesPerAttempt"] == 2
    assert request["materialType"] == "auto"

    expect(page.locator(".error")).to_contain_text("答案资料暂未完成整理")
    expect(page.get_by_role("button", name="重试整理答案")).to_be_visible()
    page.get_by_role("button", name="重试整理答案").click()
    expect(page.get_by_text("一致 1", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认这些答案与当前作业一致").click()
    expect(page.get_by_text("已确认一致", exact=True)).to_be_visible()
    answer_calls = calls(page, "exam_answer_source_analyze")
    assert len(answer_calls) == 2
    assert answer_calls[0]["args"] == {"batchId": 101, "idempotencyKey": "answer-source:101:structure:v3"}
    assert answer_calls[1]["args"]["batchId"] == 101
    assert answer_calls[1]["args"]["idempotencyKey"].startswith("answer-source:101:retry:")
    assert calls(page, "exam_answer_source_confirm_matches")[0]["args"] == {
        "batchId": 101,
        "sourceAiRunId": 901,
    }

    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()
    page.get_by_role("button", name="普通试卷").click()
    expect(page.locator(".error")).to_contain_text("模拟材料类型确认失败")
    expect(page.get_by_role("button", name="普通试卷")).to_be_visible()
    page.get_by_role("button", name="普通试卷").click()
    assert [item["args"] for item in calls(page, "exam_fixed_intake_confirm_material_type")] == [
        {"batchId": 101, "materialType": "ordinary_paper"},
        {"batchId": 101, "materialType": "ordinary_paper"},
    ]

    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()
    page.get_by_label("第一份是谁").select_option("2")
    page.get_by_text("有人缺交？点这里勾选", exact=True).click()
    absent_three = page.locator(".intake-absence-list label").filter(has_text="3号 · 王同学").locator("input")
    absent_three.check()
    expect(page.get_by_text("已标记 1 人缺交", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认这 3 份学生顺序").click()
    expect(page.locator(".error")).to_contain_text("模拟学生归组确认失败")
    expect(page.get_by_label("第一份是谁")).to_have_value("2")
    expect(absent_three).to_be_checked()
    page.get_by_role("button", name="确认这 3 份学生顺序").click()
    grouping_calls = calls(page, "exam_fixed_intake_confirm_grouping")
    assert [item["args"] for item in grouping_calls] == [
        {"batchId": 101, "firstStudentNo": "2", "absentStudentNos": ["3"]},
        {"batchId": 101, "firstStudentNo": "2", "absentStudentNos": ["3"]},
    ]

    expect(page.get_by_text("照片与学生顺序已确认", exact=True)).to_be_visible()
    expect(page.get_by_text("2号至 5号，共 3 名；后续页面质量异常只影响对应页组。", exact=True)).to_be_visible()
    expect(page.get_by_text("看一眼照片是否清楚", exact=True)).to_be_visible()
    expect(page.locator(".intake-page-thumb")).to_have_count(6)
    page.locator(".intake-page-thumb").nth(1).click()
    expect(page.get_by_text("1 页需重拍", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认清楚页面，扣住 1 页重拍").click()
    expect(page.locator(".error")).to_contain_text("模拟页面质量确认失败")
    expect(page.get_by_text("1 页需重拍", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认清楚页面，扣住 1 页重拍").click()
    quality_calls = calls(page, "exam_fixed_intake_confirm_grouping_quality")
    assert [item["args"] for item in quality_calls] == [
        {"batchId": 101, "rejectedPageIds": [302]},
        {"batchId": 101, "rejectedPageIds": [302]},
    ]

    expect(page.get_by_text("页面质量与正式归属已确认", exact=True)).to_be_visible()
    expect(page.get_by_role("button", name="2号 李同学 · 第2页重拍")).to_be_visible()
    page.get_by_role("button", name="2号 李同学 · 第2页重拍").click()
    expect(page.get_by_text(re.compile("已进入后续识别：3 名；需重拍：0 名"))).to_be_visible()
    retake_call = calls(page, "exam_fixed_intake_replace_rejected_page")[0]
    assert retake_call["args"] == {
        "batchId": 101,
        "rejectedPageId": 302,
        "replacementPath": "/tmp/RETAKE_0002.jpg",
    }

    review = page.get_by_role("button", name="进入批改终审")
    expect(review).to_be_enabled()
    review.click()
    expect(page.get_by_role("button", name="3 老师核对")).to_have_class(re.compile(r"\bactive\b"))
    assert_no_authority_writes(page)


def test_answer_conflict_choices(browser, base_url: str) -> None:
    adopt = browser.new_page(viewport={"width": 1440, "height": 1200})
    adopt.add_init_script(MOCK_SCRIPT)
    open_exam(adopt, f"{base_url}?scenario=conflict")
    prepare_with_pasted_answer(adopt)
    expect(adopt.get_by_text("冲突 1", exact=True)).to_be_visible()
    expect(adopt.get_by_text("结构未变时已按顺序预填；如有增删或重排，只需改下面的对应关系。", exact=True)).to_be_visible()
    adopt_button = adopt.get_by_role("button", name="采用答案与评分点，另存新版本")
    expect(adopt_button).to_be_enabled()
    mapping_selects = adopt.locator(".rubric-mapping-row select")
    expect(mapping_selects).to_have_count(2)
    mapping_selects.nth(1).select_option("old-point-1")
    expect(adopt_button).to_be_disabled()
    assert calls(adopt, "exam_answer_source_adopt_new_version") == []
    mapping_selects.nth(1).select_option("old-point-2")
    expect(adopt_button).to_be_enabled()
    adopt_button.click()
    expect(adopt.get_by_text(re.compile("已把 1 道冲突题保存为作业第 4 版"))).to_be_visible()
    expect(adopt.get_by_text(re.compile("本批学生照片仍按原答案批改，不会被重写"))).to_be_visible()
    adopt_call = calls(adopt, "exam_answer_source_adopt_new_version")[0]
    assert adopt_call["args"] == {
        "batchId": 101,
        "sourceAiRunId": 901,
        "rubricMappings": [
            {
                "assessmentItemId": 502,
                "candidateOrderIndex": 0,
                "action": "reuse_existing",
                "previousStableId": "old-point-1",
            },
            {
                "assessmentItemId": 502,
                "candidateOrderIndex": 1,
                "action": "reuse_existing",
                "previousStableId": "old-point-2",
            },
        ],
    }
    assert_no_authority_writes(adopt)

    keep = browser.new_page(viewport={"width": 1440, "height": 1200})
    keep.add_init_script(MOCK_SCRIPT)
    open_exam(keep, f"{base_url}?scenario=keep")
    prepare_with_pasted_answer(keep)
    keep.get_by_role("button", name="沿用当前作业答案继续").click()
    expect(keep.get_by_text("已沿用当前答案", exact=True)).to_be_visible()
    assert calls(keep, "exam_answer_source_keep_bound")[0]["args"] == {
        "batchId": 101,
        "sourceAiRunId": 901,
    }
    assert_no_authority_writes(keep)


def test_equal_options_refresh_preserves_scope(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=options_refresh&material=ordinary_paper")
    expand_new_intake(page)
    page.get_by_label("批改哪份作业").select_option("13")
    select_student_papers(page)
    page.get_by_placeholder("也可以在这里粘贴答案").fill("  刷新后仍保留  ")
    page.get_by_role("button", name="上传并开始整理").click()
    page.get_by_role("button", name="继续识别 1 页").click()
    expect(page.get_by_text("可确认 1", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认 1 页并开始批改").click()
    expect(page.get_by_text("已确认 1", exact=True)).to_be_visible()
    page.wait_for_function("window.__fixedOptionsCalls >= 2")
    expand_new_intake(page)

    expect(page.get_by_label("班级")).to_have_value("1")
    expect(page.get_by_label("批改哪份作业")).to_have_value("13")
    expect(page.get_by_text("已选 6 份", exact=True)).to_be_visible()
    expect(page.get_by_placeholder("也可以在这里粘贴答案")).to_have_value("  刷新后仍保留  ")
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()
    expect(page.get_by_text("已确认 1", exact=True)).to_be_visible()
    assert_no_authority_writes(page)


def test_ordinary_second_batch_clears_confirmations(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=ordinary_batch_reset&material=ordinary_paper")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    page.get_by_role("button", name="继续识别 1 页").click()
    expect(page.get_by_text("可确认 1", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认 1 页并开始批改").click()
    expect(page.get_by_text("已确认 1", exact=True)).to_be_visible()

    select_student_papers(page, "NEW_0001.jpg", "NEW_0004.jpg")
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_have_count(0)
    page.get_by_role("button", name="上传并开始整理").click()
    page.get_by_role("button", name="继续识别 1 页").click()
    expect(page.get_by_text("可确认 1", exact=True)).to_be_visible()
    expect(page.get_by_text("已确认 0", exact=True)).to_be_visible()
    analyzed_page_ids = [item["args"]["pageId"] for item in calls(page, "exam_ordinary_paper_analyze_page")]
    assert analyzed_page_ids == [301, 401]
    assert_no_authority_writes(page)


def test_answer_sheet_second_batch_clears_failures(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=answer_sheet_batch_reset&material=answer_sheet")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("失败页 1", exact=True)).to_be_visible()

    select_student_papers(page, "NEW_0001.jpg", "NEW_0004.jpg")
    expect(page.get_by_text("失败页 1", exact=True)).to_have_count(0)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已处理 1/1 页", exact=True)).to_be_visible()
    expect(page.get_by_text("失败页 0", exact=True)).to_be_visible()
    processed_page_ids = [item["args"]["pageId"] for item in calls(page, "exam_answer_sheet_process_page")]
    assert processed_page_ids == [301, 401]
    assert_no_authority_writes(page)


def test_dictation_second_batch_clears_processing_ui(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=dictation_batch_reset&material=dictation")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("正在处理 1 页…", exact=True)).to_be_visible()

    select_student_papers(page, "NEW_0001.jpg", "NEW_0004.jpg")
    expect(page.get_by_text("正在处理 1 页…", exact=True)).to_have_count(0)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("精确命中 1", exact=True)).to_be_visible()
    processed_page_ids = [item["args"]["pageId"] for item in calls(page, "exam_dictation_process_page")]
    assert processed_page_ids == [301, 401]
    assert_no_authority_writes(page)


def test_full_reload_fails_closed_to_upload_start(browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=route&material=ordinary_paper")
    prepare_with_pasted_answer(page)
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_be_visible()

    page.reload()
    page.wait_for_load_state("networkidle")
    enter_exam(page)
    expect(page.get_by_text("未选择", exact=True)).to_be_visible()
    expect(page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True)).to_have_count(0)
    expect(page.get_by_placeholder("也可以在这里粘贴答案")).to_have_value("")
    assert not calls(page, "exam_fixed_intake_prepare")
    assert_no_authority_writes(page)


def test_review_route(browser, base_url: str, query: str, expected_tab: str, evidence_text: str = "") -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    page.add_init_script(MOCK_SCRIPT)
    open_exam(page, f"{base_url}?scenario=route&{query}")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("页面质量与正式归属已确认", exact=True)).to_be_visible()
    if evidence_text:
        expect(page.get_by_text(evidence_text, exact=True)).to_be_visible()
    route = page.get_by_role("button", name="进入批改终审")
    expect(route).to_be_enabled()
    route.click()
    expect(page.get_by_role("button", name="3 老师核对")).to_have_class(re.compile(r"\bactive\b"))
    assert_no_authority_writes(page)


def test_material_routes(browser, base_url: str) -> None:
    test_review_route(browser, base_url, "material=ordinary_paper", "标准卷终审")
    test_review_route(
        browser,
        base_url,
        "material=answer_sheet&subjective=0",
        "标准卷终审",
        "主观区已转写 0",
    )
    test_review_route(
        browser,
        base_url,
        "material=answer_sheet&subjective=1",
        "答题卡主观题",
        "主观区已转写 1",
    )
    test_review_route(browser, base_url, "material=dictation", "默写复核", "精确命中 1")


def test_fixed_intake_shared_shell(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        test_empty_and_input_guards(browser, base_url)
        test_prepare_retry_keeps_idempotency(browser, base_url)
        test_prepare_inputs_rotate_idempotency(browser, base_url)
        test_context_switch_resets_session_but_keeps_draft(browser, base_url)
        test_answer_file_request(browser, base_url)
        test_shared_shell_and_failure_retention(browser, base_url)
        test_answer_conflict_choices(browser, base_url)
        test_equal_options_refresh_preserves_scope(browser, base_url)
        test_ordinary_second_batch_clears_confirmations(browser, base_url)
        test_answer_sheet_second_batch_clears_failures(browser, base_url)
        test_dictation_second_batch_clears_processing_ui(browser, base_url)
        test_full_reload_fails_closed_to_upload_start(browser, base_url)
        test_material_routes(browser, base_url)
        browser.close()


if __name__ == "__main__":
    test_fixed_intake_shared_shell("http://127.0.0.1:4173")
    print("fixed intake shared shell UI characterization: PASS")
