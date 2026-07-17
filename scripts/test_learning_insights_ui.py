"""M3 错题事实/老师确认错因与 M6 掌握入口浏览器冒烟测试。"""

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
    if (cmd === "wrongbook_schedule_policy") {
      return {
        id: 1, public_id: "policy-1", policy_key: "learning_default", revision: 1,
        timezone: "Asia/Shanghai", default_delay_days: 7, daily_limit_per_student: 3,
        weekend_policy: "next_workday", holiday_policy: "next_workday", max_shift_days: 60,
        state: "active", created_by: "system", created_at: "2026-07-16T07:00:00Z",
        holidays: []
      };
    }
    if (cmd === "update_wrongbook_schedule_policy") {
      const input = args.input;
      return {
        id: 2, public_id: "policy-2", policy_key: "learning_default", revision: 2,
        timezone: "Asia/Shanghai", default_delay_days: input.defaultDelayDays,
        daily_limit_per_student: input.dailyLimitPerStudent,
        weekend_policy: input.weekendPolicy, holiday_policy: input.holidayPolicy,
        max_shift_days: input.maxShiftDays, state: "active",
        created_by: "local_teacher", created_at: "2026-07-16T12:02:00Z",
        holidays: input.holidays
      };
    }
    if (cmd === "class_wrongbook_dashboard") {
      const classId = Number(args.classId);
      if (classId === 2) {
        return {
          meta: { schema_version: 4, rule_version: "m3-published-wrong-facts-v4",
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
        meta: { schema_version: 4, rule_version: "m3-published-wrong-facts-v4",
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
            latest_error_grade_decision_public_id: "decision-1",
            latest_error_publication_public_id: "publication-1",
            cause_options: [
              { code: "missing_answer", label: "未作答", description: "学生没有写出答案" },
              { code: "fact_error", label: "史实错误", description: "历史事实错误" },
              { code: "concept_confusion", label: "概念混淆", description: "相近概念混淆" },
              { code: "misread_prompt", label: "审题偏差", description: "回答方向偏差" },
              { code: "other", label: "其他", description: "老师补充" }
            ],
            cause_review: null,
            correction_assignment: null,
            reinforcement_assignment: null,
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
            latest_error_grade_decision_public_id: "decision-2",
            latest_error_publication_public_id: "publication-2",
            cause_options: [
              { code: "missing_answer", label: "未作答", description: "学生没有写出答案" },
              { code: "fact_error", label: "史实错误", description: "历史事实错误" },
              { code: "incomplete_expression", label: "表达不完整", description: "内容不完整" },
              { code: "other", label: "其他", description: "老师补充" }
            ],
            cause_review: null,
            correction_assignment: {
              public_id: "correction-2", class_id: 1, student_id: 2,
              student_no: "02", student_name: "小周",
              question_version_public_id: "qv-2",
              source_grade_decision_public_id: "decision-2",
              source_publication_public_id: "publication-2",
              assessment_public_id: "assessment-correction-2",
              assessment_version_public_id: "assessment-version-correction-2",
              assessment_title: "02号 小周 · 《南京条约》签订于____年。 · 订正",
              status: "published", latest_attempt_public_id: "attempt-correction-2",
              created_by: "local_teacher", created_at: "2026-07-14T10:00:00Z"
            },
            reinforcement_assignment: null,
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
            latest_error_grade_decision_public_id: "decision-3",
            latest_error_publication_public_id: "publication-3",
            cause_options: [
              { code: "fact_error", label: "史实错误", description: "历史事实错误" },
              { code: "concept_confusion", label: "概念混淆", description: "相近概念混淆" },
              { code: "other", label: "其他", description: "老师补充" }
            ],
            cause_review: {
              public_id: "cause-review-3", revision: 1,
              grade_decision_public_id: "decision-3",
              publication_public_id: "publication-3",
              cause_codes: ["concept_confusion"], teacher_note: null,
              confirmed_by: "local_teacher", confirmed_at: "2026-07-16T10:00:00Z"
            },
            correction_assignment: null,
            reinforcement_assignment: null,
            knowledge_nodes: [{ public_id: "k-2", title: "辛亥革命局限" }],
            ability_dimensions: []
          }
        ]
      };
    }
    if (cmd === "confirm_wrongbook_error_causes") {
      const input = args.input;
      return {
        public_id: "cause-review-new", revision: 1,
        grade_decision_public_id: input.gradeDecisionPublicId,
        publication_public_id: input.publicationPublicId,
        cause_codes: input.causeCodes,
        teacher_note: input.teacherNote ?? null,
        confirmed_by: "local_teacher",
        confirmed_at: "2026-07-16T12:10:00Z"
      };
    }
    if (cmd === "create_wrongbook_single_correction") {
      const input = args.input;
      return {
        public_id: "correction-new", class_id: input.classId, student_id: input.studentId,
        student_no: "01", student_name: "小林",
        question_version_public_id: input.questionVersionPublicId,
        source_grade_decision_public_id: input.sourceGradeDecisionPublicId,
        source_publication_public_id: input.sourcePublicationPublicId,
        assessment_public_id: "assessment-correction-new",
        assessment_version_public_id: "assessment-version-correction-new",
        assessment_title: "01号 小林 · 洋务运动失败的根本原因是？ · 订正",
        status: "waiting_upload", latest_attempt_public_id: null,
        created_by: "local_teacher", created_at: "2026-07-16T12:11:00Z"
      };
    }
    if (cmd === "preview_wrongbook_reinforcement") {
      const input = args.input;
      return {
        class_id: input.classId, student_id: input.studentId,
        student_no: "02", student_name: "小周",
        question_version_public_id: input.questionVersionPublicId,
        source_grade_decision_public_id: input.sourceGradeDecisionPublicId,
        source_publication_public_id: input.sourcePublicationPublicId,
        strategy: "same_question_recheck", priority: "normal",
        reason: "已完成一次订正；建议跨日期再次作答，验证是否保持。",
        policy_public_id: "policy-2", policy_revision: 2,
        previewed_as_of_date: "2026-07-16", corrected_on: "2026-07-15",
        earliest_due_date: "2026-07-22", suggested_due_date: "2026-07-22",
        shifted_days: 0, existing_task_count: 1, daily_limit_per_student: 4
      };
    }
    if (cmd === "confirm_wrongbook_reinforcement") {
      const input = args.input;
      return {
        public_id: "reinforcement-new", class_id: input.classId, student_id: input.studentId,
        student_no: "02", student_name: "小周",
        question_version_public_id: input.questionVersionPublicId,
        source_grade_decision_public_id: input.sourceGradeDecisionPublicId,
        source_publication_public_id: input.sourcePublicationPublicId,
        strategy: "same_question_recheck", priority: "normal",
        policy_public_id: input.expectedPolicyPublicId, policy_revision: 2,
        due_date: input.expectedDueDate,
        assessment_public_id: "assessment-reinforcement-new",
        assessment_version_public_id: "assessment-version-reinforcement-new",
        assessment_title: "02号 小周 · 《南京条约》签订于____年。 · 巩固复测",
        task_id: 19, status: "scheduled", latest_attempt_public_id: null,
        created_by: "local_teacher", created_at: "2026-07-16T12:12:00Z"
      };
    }
    if (cmd === "class_operations_dashboard") {
      return {
        meta: { schema_version: 2, rule_version: "m6.1-operations-v2",
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

        page.get_by_role("button", name="巩固规则").click()
        expect(page.get_by_text("只有老师确认后才会建立任务", exact=False)).to_be_visible()
        limit_input = page.locator(".schedule-policy-main input[type=number]").nth(1)
        limit_input.fill("4")
        page.get_by_role("button", name="保存规则").click()
        expect(page.get_by_text("当前第 2 版", exact=True)).to_be_visible()
        policy_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'update_wrongbook_schedule_policy')"
        )
        assert len(policy_calls) == 1
        assert policy_calls[0]["args"]["input"]["dailyLimitPerStudent"] == 4
        page.get_by_role("button", name="收起规则").click()

        first_item = page.locator(".wrongbook-item").first
        first_item.get_by_role("button", name="确认错因").click()
        first_item.get_by_text("史实错误", exact=True).click()
        first_item.get_by_text("概念混淆", exact=True).click()
        first_item.get_by_label("错因备注").fill("根本原因与直接原因混淆")
        first_item.get_by_role("button", name="保存错因").click()
        expect(first_item.get_by_text("老师确认错因", exact=True)).to_be_visible()
        expect(first_item.get_by_text("根本原因与直接原因混淆", exact=True)).to_be_visible()
        cause_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'confirm_wrongbook_error_causes')"
        )
        assert len(cause_calls) == 1
        assert cause_calls[0]["args"]["input"]["causeCodes"] == ["fact_error", "concept_confusion"]
        assert cause_calls[0]["args"]["input"]["gradeDecisionPublicId"] == "decision-1"

        first_item.get_by_role("button", name="建立订正").click()
        expect(first_item.get_by_text("不会修改原成绩", exact=False)).to_be_visible()
        first_item.get_by_role("button", name="确认建立").click()
        expect(first_item.get_by_text("订正已建立 · 等待上传", exact=True)).to_be_visible()
        expect(first_item.get_by_role("button", name="去上传批改")).to_be_visible()
        correction_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'create_wrongbook_single_correction')"
        )
        assert len(correction_calls) == 1
        correction_input = correction_calls[0]["args"]["input"]
        assert correction_input["classId"] == 1
        assert correction_input["studentId"] == 1
        assert correction_input["questionVersionPublicId"] == "qv-1"
        assert correction_input["sourceGradeDecisionPublicId"] == "decision-1"
        assert correction_input["sourcePublicationPublicId"] == "publication-1"

        corrected_item = page.locator(".wrongbook-item", has_text="《南京条约》签订于____年。")
        corrected_item.get_by_role("button", name="安排巩固").click()
        expect(corrected_item.get_by_text("建议 2026-07-22 再做一次", exact=True)).to_be_visible()
        before_confirm = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'confirm_wrongbook_reinforcement').length"
        )
        assert before_confirm == 0
        corrected_item.get_by_role("button", name="确认安排").click()
        expect(corrected_item.get_by_text("巩固已安排 · 2026-07-22", exact=True)).to_be_visible()
        reinforcement_calls = page.evaluate(
            "() => window.__learningCalls.filter((item) => "
            "item.cmd === 'confirm_wrongbook_reinforcement')"
        )
        assert len(reinforcement_calls) == 1
        reinforcement_input = reinforcement_calls[0]["args"]["input"]
        assert reinforcement_input["expectedPolicyPublicId"] == "policy-2"
        assert reinforcement_input["expectedDueDate"] == "2026-07-22"
        assert reinforcement_input["previewedAsOfDate"] == "2026-07-16"
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
