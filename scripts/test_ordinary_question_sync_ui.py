"""固定普通试卷印刷题自动沉淀的浏览器冒烟测试。

通过浏览器端 Tauri invoke mock 验证：
照片按顺序归组、老师确认页面、印刷题进入私有候选题库，
以及题库同步发生在客观题识别之前。
后端隐私门禁、幂等 claim 和事务边界由 Rust 测试覆盖。
"""

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__ordinaryCalls = [];
window.__qualityConfirmed = false;
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__ordinaryCalls.push({ cmd, args });
    if (cmd === "plugin:dialog|open") return ["/tmp/IMG_0001.jpg"];
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "list_profile_scope_options") return [];
    if (cmd === "class_operations_dashboard") {
      return {
        meta: {
          schema_version: 2,
          rule_version: "m6.1-operations-v2",
          calculated_at: "2026-07-19T12:00:00Z",
          as_of_date: args.asOfDate,
          recitation_watermark: null,
          exam_watermark: null
        },
        class: {
          id: 1,
          name: "八年级一班",
          term: "2026秋",
          textbook: "中国历史八上",
          enabled_student_count: 1
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
          denominator_note: "暂无任务。"
        },
        exam: {
          active_assessment_count: 1,
          expected_submission_count: 1,
          submitted_submission_count: 0,
          missing_submission_count: 1,
          ingesting_attempt_count: 0,
          grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0,
          published_submission_count: 0,
          open_pipeline_issue_count: 0,
          denominator_note: "按当前作业计算。"
        },
        students: [],
        actions: []
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
        || cmd === "exam_dictation_workbench") {
      return { rows: [], attempts: [] };
    }
    if (cmd === "exam_fixed_intake_options") {
      return [{
        classId: 1,
        className: "八年级一班",
        assessmentId: 11,
        assessmentVersionId: 12,
        assessmentTitle: "洋务运动随堂练习",
        revision: 1,
        templateVersion: "ordinary-v1",
        itemCount: 1
      }];
    }
    if (cmd === "exam_fixed_intake_infer_page_cycle") {
      return {
        expectedPagesPerAttempt: 1,
        confidence: 0.99,
        source: "visual_repeating_layout_v1",
        issueCodes: [],
        needsTeacherInput: false
      };
    }
    if (cmd === "exam_fixed_intake_prepare") {
      return {
        batchId: 101,
        batchPublicId: "batch-101",
        documents: [{
          role: "student_work",
          format: "jpeg",
          originalName: "IMG_0001.jpg",
          pageCount: 1
        }],
        studentDocumentCount: 1,
        studentPageCount: 1,
        answerDocumentCount: 0,
        route: "ready_for_batch_confirm",
        targetCount: 1,
        readyCount: 0,
        reviewCount: 1,
        blockedCount: 0,
        completedCount: 0,
        reasonCodes: ["STUDENT_RANGE_OR_ABSENCE_CONFIRMATION_REQUIRED"],
        orderPolicy: "natural_filename_then_capture_time",
        orderConfidence: 0.99,
        orderConflictCodes: [],
        materialType: "ordinary_paper",
        materialTypeDecision: "suggested",
        materialTypeConfidence: 0.98,
        materialTypeNeedsConfirmation: false,
        groupingRoute: "preview_ready",
        studentGroupCount: 1,
        groupingIssueCodes: [],
        expectedPagesPerAttempt: 1,
        pageCycleSource: "visual_repeating_layout_v1",
        pageCycleConfidence: 0.99,
        pageCycleNeedsTeacherInput: false,
        groupingRoster: [{
          studentId: 21,
          studentNo: "1",
          studentName: "张同学"
        }],
        groupingConfirmed: false,
        groupingFirstStudentNo: null,
        groupingLastStudentNo: null,
        qualityReviewCompleted: false,
        mappedGroupCount: 0,
        rejectedGroupCount: 0,
        nextAction: "确认学生顺序"
      };
    }
    if (cmd === "exam_fixed_intake_confirm_grouping") {
      return {
        groupingRoute: "preview_ready",
        studentGroupCount: 1,
        groupingIssueCodes: [],
        groupingConfirmed: true,
        groupingFirstStudentNo: "1",
        groupingLastStudentNo: "1",
        nextAction: "确认照片清晰度"
      };
    }
    if (cmd === "exam_fixed_intake_grouping_evidence") {
      return [{
        groupIndex: 0,
        studentId: 21,
        studentNo: "1",
        studentName: "张同学",
        pages: [{
          pageId: 301,
          replacedPageId: null,
          pageNo: 1,
          importIndex: 0,
          archivedPath: "/tmp/IMG_0001.jpg",
          originalName: "IMG_0001.jpg",
          pageState: window.__qualityConfirmed ? "mapped" : "prepared",
          qualityResult: window.__qualityConfirmed ? "pass" : null,
          matchDecision: window.__qualityConfirmed ? "teacher_confirmed" : "suggested"
        }]
      }];
    }
    if (cmd === "exam_fixed_intake_confirm_grouping_quality") {
      window.__qualityConfirmed = true;
      return {
        qualityReviewCompleted: true,
        mappedGroupCount: 1,
        rejectedGroupCount: 0,
        nextAction: "普通试卷版面识别"
      };
    }
    if (cmd === "exam_ordinary_paper_analyze_page") {
      return {
        ai_run_id: 401,
        status: "succeeded",
        output: {
          schema_version: 1,
          page_id: 301,
          expected_page_no: 1,
          state: "ready",
          quality: { result: "pass", issue_codes: [] },
          alignment: { confidence: 0.99 },
          regions: [{
            assessment_item_id: 501,
            region_index: 0,
            mapping_confidence: 0.99,
            mark_cells: [{ label: "A" }, { label: "B" }]
          }],
          printed_questions: [{
            assessment_item_id: 501,
            stem: "洋务运动后期提出的口号是？",
            material_text: null,
            options: [
              { label: "A", content: "自强", order_index: 0 },
              { label: "B", content: "求富", order_index: 1 }
            ],
            extraction_confidence: 0.99,
            privacy: {
              schema_version: 1,
              sanitized: true,
              student_identity_detected: false,
              student_answer_detected: false,
              teacher_mark_detected: false,
              score_detected: false
            }
          }],
          confidence: 0.99,
          issue_codes: []
        },
        failure: null
      };
    }
    if (cmd === "exam_ordinary_paper_confirm_page_structure") {
      return {
        confirmation: {
          id: 601,
          ai_run_id: 401,
          page_id: 301,
          alignment_revision_id: 611,
          region_revision_ids: [701],
          confirmed_by: "local_teacher",
          created_at: "2026-07-19T12:10:00Z"
        },
        alignment: { id: 611, decision: "teacher_confirmed" },
        regions: [{
          id: 701,
          assessment_item_id: 501,
          region_index: 0,
          decision: "teacher_confirmed"
        }]
      };
    }
    if (cmd === "exam_ordinary_paper_sync_questions") {
      if (window.location.search.includes("syncFail=1")) {
        throw new Error("模拟题库旁路失败");
      }
      return {
        schema_version: 1,
        state: "completed",
        assessment_version_id: 12,
        page_no: 1,
        source_page_id: 301,
        source_ai_run_id: 401,
        printed_question_count: 1,
        eligible_count: 1,
        enqueued_count: 1,
        matched_count: 0,
        candidate_created_count: 1,
        needs_review_count: 0,
        privacy_rejected_count: 0,
        low_confidence_skipped_count: 0,
        failed_count: 0,
        reused_existing_source: false
      };
    }
    if (cmd === "exam_objective_recognize_region") return { status: "queued" };
    return [];
  }
};
"""


def reach_ready_page(page, base_url: str) -> None:
    page.goto(base_url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()
    expect(page.get_by_text("上传后自动整理", exact=True)).to_be_visible()
    page.get_by_role("button", name="选择试卷").click()
    expect(page.get_by_text("已选 1 份", exact=True)).to_be_visible()
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认这 1 份学生顺序").click()
    expect(page.get_by_text("照片与学生顺序已确认", exact=True)).to_be_visible()
    expect(page.get_by_text("看一眼照片是否清楚", exact=True)).to_be_visible()
    page.get_by_role("button", name="照片都清楚，确认并建立归属").click()
    expect(page.get_by_text("普通试卷正在自动识别", exact=True)).to_be_visible()
    expect(page.get_by_role("button", name="确认 1 页并开始批改")).to_be_visible()


def test_ordinary_question_sync(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.add_init_script(MOCK_SCRIPT)
        reach_ready_page(page, base_url)
        page.screenshot(path="/tmp/jiaofu-ordinary-question-sync-ready.png", full_page=True)
        page.get_by_role("button", name="确认 1 页并开始批改").click()

        expect(page.locator(".ok-banner")).to_contain_text(
            "题库复用 0 题，新增私有候选 1 题，待整理 0 题"
        )
        command_order = page.evaluate(
            """window.__ordinaryCalls
              .filter((item) => item.cmd === "exam_ordinary_paper_confirm_page_structure"
                || item.cmd === "exam_ordinary_paper_sync_questions"
                || item.cmd === "exam_objective_recognize_region")
              .map((item) => item.cmd)"""
        )
        assert command_order == [
            "exam_ordinary_paper_confirm_page_structure",
            "exam_ordinary_paper_sync_questions",
            "exam_objective_recognize_region",
        ]
        sync_call = page.evaluate(
            """window.__ordinaryCalls
              .find((item) => item.cmd === "exam_ordinary_paper_sync_questions")"""
        )
        assert sync_call["args"] == {"pageId": 301, "aiRunId": 401}
        page.screenshot(path="/tmp/jiaofu-ordinary-question-sync-complete.png", full_page=True)

        failed_page = browser.new_page(viewport={"width": 1440, "height": 1000})
        failed_page.add_init_script(MOCK_SCRIPT)
        reach_ready_page(failed_page, f"{base_url}?syncFail=1")
        failed_page.get_by_role("button", name="确认 1 页并开始批改").click()
        expect(failed_page.locator(".error")).to_contain_text(
            "已完成 1 个题区识别，另有 1 项需重试"
        )
        failed_order = failed_page.evaluate(
            """window.__ordinaryCalls
              .filter((item) => item.cmd === "exam_ordinary_paper_sync_questions"
                || item.cmd === "exam_objective_recognize_region")
              .map((item) => item.cmd)"""
        )
        assert failed_order == [
            "exam_ordinary_paper_sync_questions",
            "exam_objective_recognize_region",
        ]
        failed_page.screenshot(
            path="/tmp/jiaofu-ordinary-question-sync-nonblocking-failure.png",
            full_page=True,
        )
        browser.close()


if __name__ == "__main__":
    test_ordinary_question_sync("http://127.0.0.1:4173")
    print("ordinary question sync UI smoke: PASS")
