from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""Exam 知识点 Tab 的浏览器特征测试。

通过浏览器端 Tauri invoke mock 固定当前内联 KnowledgeTab 的树展示、
输入门禁、命令参数、成功刷新和失败提示。生产组件外移前先运行本测试。
"""

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__kpCalls = [];
window.__knowledge = [
  { id: 1, subject_id: null, parent_id: null, code: "HIS-8A", name: "中国近代史" },
  { id: 2, subject_id: null, parent_id: 1, code: "HIS-YW", name: "洋务运动" }
];
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "list_profile_scope_options") return [];
    if (cmd === "class_operations_dashboard") {
      return {
        meta: {
          schema_version: 2,
          rule_version: "m6.1-operations-v2",
          calculated_at: "2026-08-02T08:00:00Z",
          as_of_date: args.asOfDate,
          recitation_watermark: null,
          exam_watermark: null
        },
        class: {
          id: 1,
          name: "八年级一班",
          term: "2026秋",
          textbook: "中国历史八上",
          enabled_student_count: 0
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
          active_assessment_count: 0,
          expected_submission_count: 0,
          submitted_submission_count: 0,
          missing_submission_count: 0,
          ingesting_attempt_count: 0,
          grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0,
          published_submission_count: 0,
          open_pipeline_issue_count: 0,
          denominator_note: "暂无作业。"
        },
        students: [],
        actions: []
      };
    }
    if (cmd === "latest_class_profile") return null;
    if (cmd === "list_class_teaching_events"
        || cmd === "list_class_teaching_inputs"
        || cmd === "list_class_action_drafts") return [];
    if (cmd === "students_list" || cmd === "questions_list" || cmd === "exam_answers_list") {
      return [];
    }
    if (cmd === "kp_list") return window.__knowledge;
    if (cmd === "exam_objective_workbench"
        || cmd === "exam_answer_sheet_subjective_workbench"
        || cmd === "exam_dictation_workbench") {
      return { rows: [], attempts: [] };
    }
    if (cmd === "exam_fixed_intake_options") return [];
    if (cmd === "kp_create") {
      window.__kpCalls.push(args);
      if (window.location.search.includes("kpFail=1")) {
        throw new Error("模拟知识点保存失败");
      }
      const created = {
        id: window.__knowledge.length + 1,
        subject_id: args.subjectId,
        parent_id: args.parentId,
        code: args.code,
        name: args.name
      };
      window.__knowledge.push(created);
      return created;
    }
    return [];
  }
};
"""


def reach_knowledge_tab(page, url: str) -> None:
    page.goto(url)
    page.wait_for_load_state("networkidle")
    navigate_exam(page)
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()
    page.get_by_role("button", name="知识点", exact=True).click()
    expect(page.get_by_text("新增知识点", exact=True)).to_be_visible()
    expect(page.get_by_text("知识点树", exact=True)).to_be_visible()


def test_knowledge_tab(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.add_init_script(MOCK_SCRIPT)
        reach_knowledge_tab(page, base_url)

        expect(page.locator(".kp-root")).to_contain_text("中国近代史")
        expect(page.locator(".kp-root code")).to_have_text("HIS-8A")
        expect(page.locator(".kp-child")).to_have_count(1)
        expect(page.locator(".kp-child")).to_contain_text("洋务运动")
        expect(page.locator(".exam-card-head .tag").filter(has_text="2 个")).to_be_visible()

        parent_select = page.get_by_label("上级板块（可选）")
        expect(parent_select.locator("option")).to_have_count(2)
        expect(parent_select.locator("option").nth(0)).to_have_text("根节点")
        expect(parent_select.locator("option").nth(1)).to_have_text("中国近代史")

        page.get_by_role("button", name="创建知识点").click()
        expect(page.locator(".error")).to_have_text("知识点名称不能为空")
        assert page.evaluate("window.__kpCalls.length") == 0

        page.get_by_label("名称").fill("戊戌变法")
        page.get_by_label("编码（可选）").fill("HIS-WX")
        parent_select.select_option("1")
        page.get_by_role("button", name="创建知识点").click()

        expect(page.locator(".ok-banner")).to_have_text("知识点已创建")
        expect(page.get_by_label("名称")).to_have_value("")
        expect(page.get_by_label("编码（可选）")).to_have_value("")
        expect(page.locator(".kp-child")).to_have_count(2)
        expect(page.locator(".kp-child").filter(has_text="戊戌变法")).to_be_visible()
        assert page.evaluate("window.__kpCalls") == [{
            "subjectId": None,
            "parentId": 1,
            "code": "HIS-WX",
            "name": "戊戌变法",
        }]
        page.screenshot(path="/tmp/jiaofu-r2-knowledge-tab-created.png", full_page=True)

        failed_page = browser.new_page(viewport={"width": 1440, "height": 1000})
        failed_page.add_init_script(MOCK_SCRIPT)
        reach_knowledge_tab(failed_page, f"{base_url}?kpFail=1")
        failed_page.get_by_label("名称").fill("辛亥革命")
        failed_page.get_by_role("button", name="创建知识点").click()

        expect(failed_page.locator(".error")).to_contain_text("模拟知识点保存失败")
        expect(failed_page.get_by_label("名称")).to_have_value("辛亥革命")
        expect(failed_page.locator(".kp-child")).to_have_count(1)
        assert failed_page.evaluate("window.__kpCalls") == [{
            "subjectId": None,
            "parentId": None,
            "code": None,
            "name": "辛亥革命",
        }]
        failed_page.screenshot(path="/tmp/jiaofu-r2-knowledge-tab-failed.png", full_page=True)
        browser.close()


if __name__ == "__main__":
    test_knowledge_tab("http://127.0.0.1:4173")
    print("exam knowledge tab UI characterization: PASS")
