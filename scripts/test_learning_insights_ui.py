"""M3-0 错题事实 / M6 掌握入口浏览器冒烟测试。"""

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__learningCalls = [];
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__learningCalls.push({ cmd, args });
    if (cmd === "classes_list") {
      return window.__NO_CLASSES__ ? [] : [
        { id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" },
        { id: 2, name: "八年级二班", term: "2026秋", textbook: "中国历史八上" }
      ];
    }
    if (cmd === "class_wrongbook_dashboard") {
      const classId = Number(args.classId);
      if (classId === 2) {
        return {
          meta: { schema_version: 1, rule_version: "m3-published-wrong-facts-v1",
            calculated_at: "2026-07-16T12:00:00Z", exam_watermark: null },
          class: { id: 2, name: "八年级二班", term: "2026秋",
            textbook: "中国历史八上", enabled_student_count: 1 },
          summary: {
            affected_student_count: 0, wrong_question_count: 0,
            needs_correction_count: 0, corrected_once_count: 0,
            rechecked_correct_count: 0, repeated_error_count: 0,
            denominator_note: "仅统计本班启用学生当前有效发布快照中的老师评分。"
          },
          items: []
        };
      }
      return {
        meta: { schema_version: 1, rule_version: "m3-published-wrong-facts-v1",
          calculated_at: "2026-07-16T12:00:00Z", exam_watermark: "2026-07-16T11:40:00Z" },
        class: { id: 1, name: "八年级一班", term: "2026秋",
          textbook: "中国历史八上", enabled_student_count: 3 },
        summary: {
          affected_student_count: 2, wrong_question_count: 3,
          needs_correction_count: 1, corrected_once_count: 1,
          rechecked_correct_count: 1, repeated_error_count: 1,
          denominator_note: "仅统计本班启用学生当前有效发布快照中的老师评分；非满分记为错题。订正一次和再次答对都只是题目事实，不等于知识点已掌握。"
        },
        items: [
          {
            student_id: 1, student_no: "01", student_name: "小林",
            question_version_id: "qv-1", question_type: "single",
            stem: "洋务运动失败的根本原因是？", status: "needs_correction",
            first_error_at: "2026-07-14T08:00:00Z", last_error_at: "2026-07-16T08:00:00Z",
            latest_response_at: "2026-07-16T08:00:00Z",
            latest_score: 0, latest_max_score: 2, latest_score_ratio: 0,
            published_response_count: 2, error_response_count: 2, repeated_error: true,
            latest_assessment_title: "近代化单元测验", latest_assessment_context: "quiz",
            knowledge_nodes: [{ public_id: "k-1", title: "洋务运动失败原因" }],
            ability_dimensions: [{ public_id: "a-1", title: "因果分析" }]
          },
          {
            student_id: 2, student_no: "02", student_name: "小周",
            question_version_id: "qv-2", question_type: "fill_blank",
            stem: "《南京条约》签订于____年。", status: "corrected_once",
            first_error_at: "2026-07-13T08:00:00Z", last_error_at: "2026-07-13T08:00:00Z",
            latest_response_at: "2026-07-15T08:00:00Z",
            latest_score: 1, latest_max_score: 1, latest_score_ratio: 1,
            published_response_count: 2, error_response_count: 1, repeated_error: false,
            latest_assessment_title: "第一单元作业", latest_assessment_context: "correction",
            knowledge_nodes: [], ability_dimensions: []
          },
          {
            student_id: 2, student_no: "02", student_name: "小周",
            question_version_id: "qv-3", question_type: "true_false",
            stem: "辛亥革命结束了中国封建制度。", status: "rechecked_correct",
            first_error_at: "2026-07-10T08:00:00Z", last_error_at: "2026-07-10T08:00:00Z",
            latest_response_at: "2026-07-16T09:00:00Z",
            latest_score: 1, latest_max_score: 1, latest_score_ratio: 1,
            published_response_count: 2, error_response_count: 1, repeated_error: false,
            latest_assessment_title: "期末复测", latest_assessment_context: "exam",
            knowledge_nodes: [{ public_id: "k-2", title: "辛亥革命局限" }],
            ability_dimensions: []
          }
        ]
      };
    }
    if (cmd === "class_operations_dashboard") {
      return {
        meta: { schema_version: 1, rule_version: "m6.1-operations-v1",
          calculated_at: "2026-07-16T12:00:00Z", as_of_date: args.asOfDate,
          recitation_watermark: null, exam_watermark: null },
        class: { id: 1, name: "八年级一班", term: "2026秋",
          textbook: "中国历史八上", enabled_student_count: 3 },
        recitation: {
          expected_student_count: 0, completed_student_count: 0, expected_task_count: 0,
          confirmed_task_count: 0, submitted_task_count: 0, not_submitted_student_count: 0,
          pending_teacher_review_count: 0, overdue_pending_review_count: 0,
          recognition_failure_count: 0, recognition_processing_count: 0,
          denominator_note: "无"
        },
        exam: {
          active_assessment_count: 0, expected_submission_count: 0, submitted_submission_count: 0,
          missing_submission_count: 0, ingesting_attempt_count: 0, grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0, published_submission_count: 0,
          open_pipeline_issue_count: 0, denominator_note: "无"
        },
        students: [], actions: []
      };
    }
    if (cmd === "exam_objective_workbench"
        || cmd === "exam_answer_sheet_subjective_workbench"
        || cmd === "exam_dictation_workbench") {
      return { rows: [], attempts: [] };
    }
    if (cmd === "students_list" || cmd === "contents_list" || cmd === "questions_list"
        || cmd === "kp_list" || cmd === "exam_answers_list"
        || cmd === "exam_fixed_intake_options" || cmd === "anomalies_list"
        || cmd === "recognition_failures_list") return [];
    return [];
  }
};
"""


def test_learning_insights(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        page.locator(".mod-row", has_text="错题与掌握").click()
        expect(page.get_by_role("heading", name="错题与掌握")).to_be_visible()
        expect(page.get_by_text("订正一次 ≠ 已掌握", exact=False)).to_be_visible()
        expect(page.get_by_role("button", name="掌握分析 · 待验证")).to_be_disabled()
        expect(page.locator(".learning-stat")).to_have_count(4)
        expect(page.locator(".wrongbook-item")).to_have_count(3)
        expect(page.get_by_text("洋务运动失败的根本原因是？", exact=True)).to_be_visible()
        expect(page.locator(".wrongbook-item .bad-text", has_text="重复出错")).to_be_visible()
        expect(page.get_by_text("尚未绑定已确认的知识点或能力", exact=False)).to_be_visible()
        page.screenshot(path="/tmp/jiaofu-learning-insights.png", full_page=True)

        page.get_by_label("筛选订正状态").select_option("needs_correction")
        expect(page.locator(".wrongbook-item")).to_have_count(1)
        page.get_by_label("筛选订正状态").select_option("all")
        page.get_by_label("筛选学生").select_option("2")
        expect(page.locator(".wrongbook-item")).to_have_count(2)

        page.get_by_label("筛选学生").select_option("all")
        page.get_by_role("button", name="去题目批改").click()
        expect(page.get_by_role("heading", name="题目批改")).to_be_visible()

        page.locator(".mod-row", has_text="错题与掌握").click()
        page.locator(".learning-scope select").select_option("2")
        expect(page.get_by_text("当前没有符合口径的已发布错题。", exact=True)).to_be_visible()

        narrow = browser.new_page(viewport={"width": 760, "height": 900})
        narrow.add_init_script(MOCK_SCRIPT)
        narrow.goto(base_url)
        narrow.wait_for_load_state("networkidle")
        narrow.locator(".mod-row", has_text="错题与掌握").click()
        expect(narrow.locator(".wrongbook-item")).to_have_count(3)
        narrow.screenshot(path="/tmp/jiaofu-learning-insights-narrow.png", full_page=True)

        empty = browser.new_page(viewport={"width": 1100, "height": 800})
        empty.add_init_script("window.__NO_CLASSES__ = true;")
        empty.add_init_script(MOCK_SCRIPT)
        empty.goto(base_url)
        empty.wait_for_load_state("networkidle")
        empty.locator(".mod-row", has_text="错题与掌握").click()
        expect(empty.locator(".empty-state")).to_contain_text("暂无班级")
        expect(empty.get_by_role("button", name="去建立班级")).to_be_visible()

        browser.close()


if __name__ == "__main__":
    test_learning_insights("http://127.0.0.1:4173")
    print("learning insights UI smoke: PASS")
