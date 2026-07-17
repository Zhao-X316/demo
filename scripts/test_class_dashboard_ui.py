"""M6.1-1 班级运行仪表盘浏览器冒烟测试。

通过浏览器端 Tauri invoke mock 验证页面布局、班级切换和跨模块跳转；
后端 SQL 口径由 Rust 单元测试覆盖。
"""

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__dashboardCalls = [];
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__dashboardCalls.push({ cmd, args });
    if (cmd === "classes_list") {
      return window.__NO_CLASSES__ ? [] : [
        { id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" },
        { id: 2, name: "八年级二班", term: "2026秋", textbook: "中国历史八上" }
      ];
    }
    if (cmd === "class_operations_dashboard") {
      const classId = Number(args.classId);
      if (classId === 2) {
        return {
          meta: {
            schema_version: 2, rule_version: "m6.1-operations-v2",
            calculated_at: "2026-07-16T12:00:00Z", as_of_date: args.asOfDate,
            recitation_watermark: null, exam_watermark: null
          },
          class: { id: 2, name: "八年级二班", term: "2026秋", textbook: "中国历史八上", enabled_student_count: 1 },
          recitation: {
            expected_student_count: 0, completed_student_count: 0, expected_task_count: 0,
            confirmed_task_count: 0, submitted_task_count: 0, not_submitted_student_count: 0,
            pending_teacher_review_count: 0, overdue_pending_review_count: 0,
            recognition_failure_count: 0, recognition_processing_count: 0,
            denominator_note: "分母为所选日期有至少一项有效背诵任务的启用学生。"
          },
          exam: {
            active_assessment_count: 0, expected_submission_count: 0, submitted_submission_count: 0,
            missing_submission_count: 0, ingesting_attempt_count: 0, grading_attempt_count: 0,
            ready_to_publish_attempt_count: 0, published_submission_count: 0,
            open_pipeline_issue_count: 0,
            denominator_note: "分母为当前 active 作业数 × 班级启用学生数。"
          },
          students: [{
            student_id: 4, student_no: "04", student_name: "小吴",
            recitation_status: "not_scheduled", recitation_due_task_count: 0,
            recitation_confirmed_task_count: 0, exam_status: "not_assigned",
            exam_expected_submission_count: 0, exam_submitted_submission_count: 0,
            exam_published_submission_count: 0
          }],
          actions: []
        };
      }
      return {
        meta: {
          schema_version: 2, rule_version: "m6.1-operations-v2",
          calculated_at: "2026-07-16T12:00:00Z", as_of_date: args.asOfDate,
          recitation_watermark: "2026-07-16T11:30:00Z", exam_watermark: "2026-07-16T11:45:00Z"
        },
        class: { id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上", enabled_student_count: 2 },
        recitation: {
          expected_student_count: 2, completed_student_count: 1, expected_task_count: 3,
          confirmed_task_count: 2, submitted_task_count: 2, not_submitted_student_count: 1,
          pending_teacher_review_count: 1, overdue_pending_review_count: 1,
          recognition_failure_count: 1, recognition_processing_count: 0,
          denominator_note: "分母为所选日期有至少一项有效背诵任务的启用学生；该生当日全部任务终审后才计为完成。"
        },
        exam: {
          active_assessment_count: 1, expected_submission_count: 2, submitted_submission_count: 1,
          missing_submission_count: 1, ingesting_attempt_count: 0, grading_attempt_count: 1,
          ready_to_publish_attempt_count: 1, published_submission_count: 0,
          open_pipeline_issue_count: 1,
          denominator_note: "分母为当前 active 作业数 × 班级启用学生数；M2 尚无截止日期，因此这里不称为“今日作业”。"
        },
        students: [
          {
            student_id: 1, student_no: "01", student_name: "小林",
            recitation_status: "completed", recitation_due_task_count: 2,
            recitation_confirmed_task_count: 2, exam_status: "ready_to_publish",
            exam_expected_submission_count: 1, exam_submitted_submission_count: 1,
            exam_published_submission_count: 0
          },
          {
            student_id: 2, student_no: "02", student_name: "小周",
            recitation_status: "not_submitted", recitation_due_task_count: 1,
            recitation_confirmed_task_count: 0, exam_status: "not_submitted",
            exam_expected_submission_count: 1, exam_submitted_submission_count: 0,
            exam_published_submission_count: 0
          }
        ],
        actions: [
          {
            kind: "recitation_recognition_failed", title: "处理背诵识别失败",
            detail: "进入背诵批改台重试、重新定位或作废失败录音。", count: 1,
            severity: "blocking", target_module: "recitation", target_view: "desk"
          },
          {
            kind: "exam_pipeline_issue", title: "处理作业导入异常",
            detail: "进入题目批改，核对图片质量、学生匹配、页码或题区。", count: 1,
            severity: "blocking", target_module: "exam", target_view: "exam"
          }
        ]
      };
    }
    if (cmd === "day_rollover") return { rolled: 0, reviews: 0 };
    if (cmd === "dashboard_today") {
      return {
        date: "2026-07-16",
        summary: { should: 0, submitted: 0, passed: 0, failed: 0, pending: 0, makeup: 0, contents: [] },
        normal: [], makeup: [], review: [], overdue_review: []
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
        || cmd === "recognition_failures_list") {
      return [];
    }
    return [];
  }
};
"""


def test_dashboard(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        expect(page.get_by_role("heading", name="班级概览")).to_be_visible()
        expect(page.locator(".dashboard-stat")).to_have_count(5)
        expect(page.get_by_text("1 / 2 人", exact=False)).to_be_visible()
        expect(page.get_by_text("处理背诵识别失败", exact=True)).to_be_visible()
        expect(page.locator(".dashboard-students tbody tr")).to_have_count(2)
        expect(page.get_by_text("未提交、识别失败和证据不足都不是“能力差”", exact=False)).to_be_visible()
        page.screenshot(path="/tmp/jiaofu-class-dashboard.png", full_page=True)

        page.locator(".dashboard-scope select").select_option("2")
        expect(page.locator(".dashboard-context b")).to_have_text("八年级二班")
        expect(page.get_by_text("当前没有待处理项。", exact=True)).to_be_visible()

        page.locator(".dashboard-scope select").select_option("1")
        expect(page.locator(".dashboard-context b")).to_have_text("八年级一班")
        page.get_by_text("处理背诵识别失败", exact=True).click()
        expect(page.get_by_role("heading", name="批改台")).to_be_visible()

        page.locator("button.nav", has_text="班级概览").click()
        expect(page.get_by_role("heading", name="班级概览")).to_be_visible()
        page.get_by_text("当前作业已上传", exact=True).click()
        expect(page.get_by_role("heading", name="题目批改")).to_be_visible()

        narrow = browser.new_page(viewport={"width": 760, "height": 900})
        narrow.add_init_script(MOCK_SCRIPT)
        narrow.goto(base_url)
        narrow.wait_for_load_state("networkidle")
        expect(narrow.get_by_role("heading", name="班级概览")).to_be_visible()
        expect(narrow.locator(".dashboard-stat")).to_have_count(5)
        narrow.screenshot(path="/tmp/jiaofu-class-dashboard-narrow.png", full_page=True)

        empty = browser.new_page(viewport={"width": 1100, "height": 800})
        empty.add_init_script("window.__NO_CLASSES__ = true;")
        empty.add_init_script(MOCK_SCRIPT)
        empty.goto(base_url)
        empty.wait_for_load_state("networkidle")
        expect(empty.locator(".empty-state")).to_contain_text("暂无班级")
        expect(empty.get_by_role("button", name="去建立班级")).to_be_visible()

        browser.close()


if __name__ == "__main__":
    test_dashboard("http://127.0.0.1:4173")
    print("class dashboard UI smoke: PASS")
