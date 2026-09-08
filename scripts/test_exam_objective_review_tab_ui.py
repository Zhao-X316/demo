from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""固定标准卷客观题终审 Tab 的筛选、证据、终审、重试和发布行为。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_exam_knowledge_tab_ui import MOCK_SCRIPT as BASE_MOCK_SCRIPT


OBJECTIVE_MOCK_SCRIPT = BASE_MOCK_SCRIPT + r"""
window.__objectiveCalls = [];
window.__objectiveRows = [];
window.__objectiveAttempts = [];

function objectiveRow(overrides = {}) {
  return {
    assessment_id: 1,
    assessment_version_id: 100,
    assessment_title: "八上第一单元检测",
    attempt_id: 5001,
    attempt_state: "grading",
    active_publication_id: null,
    student_id: 1,
    student_no: "01",
    student_name: "默认同学",
    assessment_item_id: 1001,
    order_index: 1,
    max_score: 2,
    question_version_id: 7001,
    question_no: "1",
    question_type: "single",
    question_stem: "洋务运动前期的口号是？",
    answer_region_revision_id: 8001,
    crop_path: "/tmp/objective-default.png",
    suggestion_id: 9001,
    observation_state: "recognized",
    observed_answer_json: JSON.stringify({ selected_labels: ["A"] }),
    confidence: 0.98,
    suggestion_outcome: "correct",
    suggested_score: 2,
    batch_eligible: true,
    exclusion_reason: null,
    grade_decision_id: null,
    grade_decision_revision: null,
    teacher_score: null,
    confirmation_level: null,
    review_mode: null,
    current_suggestion_confirmed: false,
    decided_at: null,
    ...overrides,
  };
}

function objectiveAttempt(overrides = {}) {
  return {
    assessment_id: 1,
    assessment_version_id: 100,
    assessment_title: "八上第一单元检测",
    attempt_id: 5001,
    attempt_state: "grading",
    active_publication_id: null,
    student_id: 1,
    student_no: "01",
    student_name: "默认同学",
    item_count: 2,
    observed_count: 2,
    confirmed_count: 1,
    teacher_total_score: 2,
    max_total_score: 4,
    published_total_score: null,
    can_publish: false,
    ...overrides,
  };
}

window.__objectiveRows = [
  objectiveRow({
    student_id: 2,
    student_no: "2",
    student_name: "二号异常",
    attempt_id: 5002,
    answer_region_revision_id: 8002,
    crop_path: null,
    suggestion_id: 9002,
    observation_state: "altered",
    observed_answer_json: JSON.stringify({ selected_values: [true, false] }),
    confidence: 0.76,
    suggestion_outcome: "unscored",
    suggested_score: null,
    batch_eligible: false,
    exclusion_reason: "OBSERVATION_NOT_RECOGNIZED",
  }),
  objectiveRow({
    student_id: 3,
    student_no: "3",
    student_name: "三号已确认",
    attempt_id: 5003,
    answer_region_revision_id: 8003,
    suggestion_id: 9003,
    confidence: 0.99,
    grade_decision_id: 9303,
    grade_decision_revision: 1,
    teacher_score: 2,
    confirmation_level: "teacher_accepted",
    review_mode: "strict_batch",
    current_suggestion_confirmed: true,
    decided_at: "2026-08-02T09:00:00Z",
  }),
  objectiveRow({
    student_id: 10,
    student_no: "10",
    student_name: "十号高置信",
    attempt_id: 5010,
    answer_region_revision_id: 8010,
    suggestion_id: 9010,
  }),
  objectiveRow({
    student_id: 11,
    student_no: "11",
    student_name: "十一号识别失败",
    attempt_id: 5011,
    answer_region_revision_id: 8011,
    crop_path: null,
    suggestion_id: 9011,
    observation_state: "failed",
    observed_answer_json: null,
    confidence: null,
    suggestion_outcome: "unscored",
    suggested_score: null,
    batch_eligible: false,
    exclusion_reason: "OBSERVATION_NOT_RECOGNIZED",
  }),
  objectiveRow({
    student_id: 12,
    student_no: "12",
    student_name: "十二号判断题",
    attempt_id: 5012,
    assessment_item_id: 1002,
    order_index: 2,
    max_score: 1,
    question_version_id: 7002,
    question_no: "2",
    question_type: "true_false",
    question_stem: "洋务运动使中国走上富强道路。",
    answer_region_revision_id: 8012,
    suggestion_id: 9012,
    observed_answer_json: JSON.stringify({ selected: true }),
    suggestion_outcome: "incorrect",
    suggested_score: 0,
    batch_eligible: false,
    exclusion_reason: "NOT_BATCH_ELIGIBLE",
  }),
  objectiveRow({
    assessment_id: 2,
    assessment_version_id: 200,
    assessment_title: "八上第二单元检测",
    student_id: 20,
    student_no: "20",
    student_name: "二十号第二作业",
    attempt_id: 5020,
    assessment_item_id: 2001,
    order_index: 1,
    max_score: 2,
    question_version_id: 7201,
    question_no: "1",
    question_type: "multiple",
    question_stem: "下列属于戊戌变法内容的是？",
    answer_region_revision_id: 8020,
    suggestion_id: 9020,
    observed_answer_json: JSON.stringify({ selected_labels: ["A", "C"] }),
  }),
];

window.__objectiveAttempts = [
  objectiveAttempt({
    attempt_id: 5002,
    student_id: 2,
    student_no: "2",
    student_name: "二号异常",
    confirmed_count: 2,
    teacher_total_score: 3,
    can_publish: true,
    attempt_state: "ready_to_publish",
  }),
  objectiveAttempt({
    attempt_id: 5003,
    student_id: 3,
    student_no: "3",
    student_name: "三号已确认",
    confirmed_count: 1,
    teacher_total_score: 2,
  }),
  objectiveAttempt({
    assessment_id: 2,
    assessment_version_id: 200,
    assessment_title: "八上第二单元检测",
    attempt_id: 5020,
    student_id: 20,
    student_no: "20",
    student_name: "二十号第二作业",
    item_count: 1,
    observed_count: 1,
    confirmed_count: 0,
    teacher_total_score: 0,
    max_total_score: 2,
  }),
];

function currentObjectiveWorkbench() {
  if (window.location.search.includes("objectiveEmpty=1")) {
    return { rows: [], attempts: [] };
  }
  return {
    rows: window.__objectiveRows.map((row) => ({ ...row })),
    attempts: window.__objectiveAttempts.map((attempt) => ({ ...attempt })),
  };
}

function objectiveFailureRequested() {
  return window.location.search.includes("objectiveFail=1");
}

function confirmObjectiveRow(row, score, confirmationLevel, reviewMode) {
  row.current_suggestion_confirmed = true;
  row.teacher_score = score;
  row.confirmation_level = confirmationLevel;
  row.review_mode = reviewMode;
  row.grade_decision_id = 9300 + row.suggestion_id;
  row.grade_decision_revision = 1;
  row.decided_at = "2026-08-02T10:00:00Z";
}

const __baseInvokeForObjectiveReview = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "exam_objective_workbench") return currentObjectiveWorkbench();

  if (cmd === "exam_objective_strict_batch_accept") {
    window.__objectiveCalls.push({ cmd, args });
    if (objectiveFailureRequested()) throw new Error("模拟严格批量终审失败");
    const eligible = window.__objectiveRows.filter((row) => (
      args.suggestionIds.includes(row.suggestion_id)
      && row.batch_eligible
      && !row.current_suggestion_confirmed
    ));
    eligible.forEach((row) => confirmObjectiveRow(row, row.suggested_score, "teacher_accepted", "strict_batch"));
    return {
      id: 9501,
      requested_count: args.suggestionIds.length,
      confirmed_count: eligible.length,
      excluded_count: args.suggestionIds.length - eligible.length,
      items: args.suggestionIds.map((suggestionId) => ({
        suggestion_id: suggestionId,
        outcome: eligible.some((row) => row.suggestion_id === suggestionId) ? "confirmed" : "excluded",
        reason_code: eligible.some((row) => row.suggestion_id === suggestionId) ? null : "NOT_BATCH_ELIGIBLE",
        grade_decision_id: eligible.some((row) => row.suggestion_id === suggestionId) ? 9300 + suggestionId : null,
      })),
    };
  }

  if (cmd === "exam_objective_accept") {
    window.__objectiveCalls.push({ cmd, args });
    if (objectiveFailureRequested()) throw new Error("模拟逐条接受失败");
    const row = window.__objectiveRows.find((item) => item.suggestion_id === args.suggestionId);
    confirmObjectiveRow(row, row.suggested_score, "teacher_accepted", "single");
    return { id: row.grade_decision_id, revision: 1, teacher_score: row.teacher_score };
  }

  if (cmd === "exam_objective_correct") {
    window.__objectiveCalls.push({ cmd, args });
    if (objectiveFailureRequested()) throw new Error("模拟人工记分失败");
    const row = window.__objectiveRows.find((item) => item.suggestion_id === args.suggestionId);
    confirmObjectiveRow(row, args.teacherScore, "teacher_corrected", "single");
    return { id: row.grade_decision_id, revision: 1, teacher_score: row.teacher_score };
  }

  if (cmd === "exam_objective_recognize_region") {
    window.__objectiveCalls.push({ cmd, args });
    if (objectiveFailureRequested()) throw new Error("模拟重新整理失败");
    return { state: "recognized" };
  }

  if (cmd === "exam_objective_publish_attempt") {
    window.__objectiveCalls.push({ cmd, args });
    if (objectiveFailureRequested()) throw new Error("模拟整卷发布失败");
    const attempt = window.__objectiveAttempts.find((item) => item.attempt_id === args.attemptId);
    attempt.attempt_state = "published";
    attempt.active_publication_id = 9701;
    attempt.published_total_score = attempt.teacher_total_score;
    attempt.can_publish = false;
    return {
      id: 9701,
      attempt_id: attempt.attempt_id,
      revision: 2,
      total_score: attempt.teacher_total_score,
      state: "active",
    };
  }

  return __baseInvokeForObjectiveReview(cmd, args);
};
"""


def reach_objective_tab(page: Page, url: str) -> None:
    page.goto(url)
    page.wait_for_load_state("networkidle")
    navigate_exam(page)
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()
    page.get_by_role("button", name="标准卷终审", exact=True).click()
    expect(page.get_by_text("本题证据", exact=False)).to_be_visible()
    expect(page.get_by_text("整卷发布", exact=False)).to_be_visible()


def objective_row(page: Page, student_name: str):
    return page.locator(".objective-review-row").filter(has_text=student_name)


def test_objective_review_success(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1800})
        page.add_init_script(OBJECTIVE_MOCK_SCRIPT)
        reach_objective_tab(page, base_url)

        assessment_select = page.get_by_label("作业版本")
        item_select = page.get_by_label("按题终审")
        expect(assessment_select.locator("option")).to_have_count(2)
        expect(item_select.locator("option")).to_have_count(2)
        expect(item_select.locator("option").first).to_contain_text("第 1 题 · 单选")
        expect(item_select.locator("option").nth(1)).to_contain_text("第 2 题 · 判断")

        stats = page.locator(".objective-stats")
        expect(stats).to_contain_text("4 份作答")
        expect(stats).to_contain_text("1 已确认")
        expect(stats).to_contain_text("1 可严格批量")
        expect(stats).to_contain_text("2 需单独处理")
        expect(page.locator(".objective-batch-note")).to_contain_text("阈值固定为 0.95")
        expect(page.locator(".objective-batch-note")).to_contain_text("空白、涂改、低置信度、识别失败和已终审记录都会明确排除")

        names = page.locator(".objective-student b").all_text_contents()
        assert names == ["二号异常", "三号已确认", "十号高置信", "十一号识别失败"]
        altered = objective_row(page, "二号异常")
        expect(altered).to_contain_text("无答案裁剪")
        expect(altered).to_contain_text("存在涂改")
        expect(altered).to_contain_text("√、×（冲突）")
        expect(altered).to_contain_text("识别状态不合格")
        expect(altered).to_contain_text("机器无法计分")
        confirmed = objective_row(page, "三号已确认")
        expect(confirmed).to_contain_text("老师终审 2 分 · 严格批量")
        expect(objective_row(page, "十号高置信").locator("img.objective-crop")).to_have_attribute(
            "src", "/tmp/objective-default.png"
        )
        failed = objective_row(page, "十一号识别失败")
        expect(failed.get_by_role("button", name="重新整理本题")).to_be_visible()

        page.get_by_role("button", name="确认 1 条高置信度结果").click()
        expect(page.locator(".ok-banner")).to_have_text("严格批量终审完成：确认 1 条，排除 3 条异常")
        expect(stats).to_contain_text("2 已确认")
        expect(stats).to_contain_text("0 可严格批量")
        batch_call = page.evaluate("window.__objectiveCalls[0]")
        assert batch_call["cmd"] == "exam_objective_strict_batch_accept"
        assert batch_call["args"]["suggestionIds"] == [9002, 9003, 9010, 9011]
        assert batch_call["args"]["confidenceThreshold"] == 0.95
        assert batch_call["args"]["idempotencyKey"].startswith("objective-review-")

        altered.get_by_text("人工记分", exact=True).click()
        score_input = altered.get_by_label("得分（满分 2）")
        note_input = altered.get_by_label("证据依据")
        score_input.fill("3")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".error")).to_have_text("人工得分必须位于 0~2 分")
        assert page.evaluate("window.__objectiveCalls.length") == 1

        score_input.fill("1")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".error")).to_have_text("人工记分必须填写查看原图后的证据依据")
        assert page.evaluate("window.__objectiveCalls.length") == 1

        note_input.fill(" 查看原图后确认涂改为 B ")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".ok-banner")).to_have_text("已人工确认 二号异常 的第 1 题为 1 分")
        assert page.evaluate("window.__objectiveCalls[1]") == {
            "cmd": "exam_objective_correct",
            "args": {
                "suggestionId": 9002,
                "teacherScore": 1,
                "teacherNote": "查看原图后确认涂改为 B",
            },
        }

        failed.get_by_role("button", name="重新整理本题").click()
        expect(page.locator(".ok-banner")).to_have_text("已重新整理 十一号识别失败 的第 1 题；机器结果仍需老师确认")
        retry_call = page.evaluate("window.__objectiveCalls[2]")
        assert retry_call["cmd"] == "exam_objective_recognize_region"
        assert retry_call["args"]["answerRegionRevisionId"] == 8011
        assert retry_call["args"]["idempotencyKey"].startswith("objective-omr-8011-")

        item_select.select_option("1002")
        expect(page.locator(".objective-review-row")).to_have_count(1)
        true_false = objective_row(page, "十二号判断题")
        expect(true_false).to_contain_text("正确（√）")
        expect(true_false).to_contain_text("建议错误")
        true_false.get_by_role("button", name="接受本条建议").click()
        expect(page.locator(".ok-banner")).to_have_text("已确认 十二号判断题 的第 2 题，成绩仍需整卷显式发布")
        assert page.evaluate("window.__objectiveCalls[3]") == {
            "cmd": "exam_objective_accept",
            "args": {"suggestionId": 9012},
        }

        assessment_select.select_option("200")
        expect(item_select.locator("option")).to_have_count(1)
        expect(item_select).to_have_value("2001")
        expect(page.locator(".objective-review-row")).to_have_count(1)
        expect(objective_row(page, "二十号第二作业")).to_contain_text("A、C")
        assessment_select.select_option("100")

        publish_card = page.locator(".objective-attempt").filter(has_text="二号异常")
        publish_button = publish_card.get_by_role("button", name="确认发布整卷")
        page.once("dialog", lambda dialog: dialog.dismiss())
        publish_button.click()
        assert page.evaluate("window.__objectiveCalls.length") == 4

        page.once("dialog", lambda dialog: dialog.accept())
        publish_button.click()
        expect(page.locator(".ok-banner")).to_have_text("二号异常 的成绩已发布：3 分（发布 revision 2）")
        assert page.evaluate("window.__objectiveCalls[4]") == {
            "cmd": "exam_objective_publish_attempt",
            "args": {"attemptId": 5002},
        }
        expect(publish_card).to_contain_text("已发布")
        expect(publish_card).to_contain_text("当前已发布总分：3")

        write_commands = page.evaluate("window.__objectiveCalls.map((item) => item.cmd)")
        assert write_commands == [
            "exam_objective_strict_batch_accept",
            "exam_objective_correct",
            "exam_objective_recognize_region",
            "exam_objective_accept",
            "exam_objective_publish_attempt",
        ]
        page.screenshot(path="/tmp/jiaofu-r2-objective-review-success.png", full_page=True)
        browser.close()


def test_objective_review_failures(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1800})
        page.add_init_script(OBJECTIVE_MOCK_SCRIPT)
        reach_objective_tab(page, f"{base_url}?objectiveFail=1")

        stats = page.locator(".objective-stats")
        page.get_by_role("button", name="确认 1 条高置信度结果").click()
        expect(page.locator(".error")).to_contain_text("模拟严格批量终审失败")
        expect(stats).to_contain_text("1 已确认")
        expect(stats).to_contain_text("1 可严格批量")

        altered = objective_row(page, "二号异常")
        altered.get_by_text("人工记分", exact=True).click()
        altered.get_by_label("得分（满分 2）").fill("1")
        altered.get_by_label("证据依据").fill("保留这段老师依据")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".error")).to_contain_text("模拟人工记分失败")
        expect(altered.get_by_label("得分（满分 2）")).to_have_value("1")
        expect(altered.get_by_label("证据依据")).to_have_value("保留这段老师依据")
        expect(altered).to_contain_text("需单独处理")

        failed = objective_row(page, "十一号识别失败")
        failed.get_by_role("button", name="重新整理本题").click()
        expect(page.locator(".error")).to_contain_text("模拟重新整理失败")
        expect(failed.get_by_role("button", name="重新整理本题")).to_be_visible()

        page.get_by_label("按题终审").select_option("1002")
        true_false = objective_row(page, "十二号判断题")
        true_false.get_by_role("button", name="接受本条建议").click()
        expect(page.locator(".error")).to_contain_text("模拟逐条接受失败")
        expect(true_false).to_contain_text("需单独处理")
        expect(true_false.get_by_role("button", name="接受本条建议")).to_be_enabled()

        publish_card = page.locator(".objective-attempt").filter(has_text="二号异常")
        page.once("dialog", lambda dialog: dialog.accept())
        publish_card.get_by_role("button", name="确认发布整卷").click()
        expect(page.locator(".error")).to_contain_text("模拟整卷发布失败")
        expect(publish_card).to_contain_text("待发布")
        expect(publish_card.get_by_role("button", name="确认发布整卷")).to_be_enabled()

        assert page.evaluate("window.__objectiveCalls.map((item) => item.cmd)") == [
            "exam_objective_strict_batch_accept",
            "exam_objective_correct",
            "exam_objective_recognize_region",
            "exam_objective_accept",
            "exam_objective_publish_attempt",
        ]
        expect(page.locator(".ok-banner")).to_have_count(0)
        page.screenshot(path="/tmp/jiaofu-r2-objective-review-failures.png", full_page=True)
        browser.close()


def test_objective_review_empty(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        page.add_init_script(OBJECTIVE_MOCK_SCRIPT)
        page.goto(f"{base_url}?objectiveEmpty=1")
        page.wait_for_load_state("networkidle")
        navigate_exam(page)
        page.get_by_role("button", name="标准卷终审", exact=True).click()

        empty = page.locator(".objective-empty:visible")
        expect(empty).to_contain_text("还没有可终审的标准卷客观题")
        expect(empty).to_contain_text("真实视觉识别只处理已完成学生匹配、页面配准、答案区域和答题格确认的数据")
        expect(empty).to_contain_text("需要临时录入时可继续使用“快速批改”兜底")
        expect(page.locator(".objective-review-row")).to_have_count(0)
        expect(page.get_by_role("button", name="确认发布整卷")).to_have_count(0)
        assert page.evaluate("window.__objectiveCalls") == []
        browser.close()


if __name__ == "__main__":
    test_objective_review_success("http://127.0.0.1:4173")
    test_objective_review_failures("http://127.0.0.1:4173")
    test_objective_review_empty("http://127.0.0.1:4173")
    print("exam ObjectiveReviewTab UI characterization: PASS")
