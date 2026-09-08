from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""Exam 手工录题 Tab 的浏览器特征测试。

固定当前内联 QuestionTab 的必填门禁、题型分支、选项解析、
question_create 参数、成功刷新和失败保留行为。
"""

from playwright.sync_api import expect, sync_playwright

from test_exam_knowledge_tab_ui import MOCK_SCRIPT as BASE_MOCK_SCRIPT


QUESTION_MOCK_SCRIPT = BASE_MOCK_SCRIPT + r"""
window.__questionCalls = [];
window.__questions = [
  {
    id: 7,
    subject_id: null,
    question_no: "HIS-001",
    qtype: "single",
    stem: "洋务运动前期口号是？",
    image_path: null,
    correct_answer: "A",
    knowledge_point_id: 2,
    difficulty: null,
    analysis: null,
    max_score: 2,
    enabled: true
  }
];
const baseInvoke = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "questions_list") return window.__questions;
  if (cmd === "question_create") {
    window.__questionCalls.push(args);
    if (window.location.search.includes("questionFail=1")) {
      throw new Error("模拟题目保存失败");
    }
    const created = { id: window.__questions.length + 7, ...args.q };
    window.__questions.push(created);
    return created.id;
  }
  return baseInvoke(cmd, args);
};
"""


def reach_question_tab(page, url: str) -> None:
    page.goto(url)
    page.wait_for_load_state("networkidle")
    navigate_exam(page)
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()
    page.get_by_role("button", name="题库", exact=True).click()
    expect(page.get_by_text("手工录题", exact=True)).to_be_visible()


def expected_question_input() -> dict:
    return {
        "subject_id": None,
        "question_no": "his-002",
        "qtype": "multi",
        "stem": "洋务运动的口号",
        "image_path": None,
        "correct_answer": "a, c",
        "knowledge_point_id": 1,
        "difficulty": None,
        "analysis": None,
        "max_score": 4,
        "enabled": True,
        "options": [
            {
                "label": "A",
                "content": "自强",
                "is_correct": True,
                "knowledge_point_id": 1,
                "analysis": None,
                "ord": 0,
            },
            {
                "label": "B",
                "content": "求富",
                "is_correct": False,
                "knowledge_point_id": 1,
                "analysis": None,
                "ord": 1,
            },
            {
                "label": "C",
                "content": "无前缀选项",
                "is_correct": True,
                "knowledge_point_id": 1,
                "analysis": None,
                "ord": 2,
            },
        ],
    }


def test_question_tab(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1100})
        page.add_init_script(QUESTION_MOCK_SCRIPT)
        reach_question_tab(page, base_url)

        question_items = page.locator(".question-item")
        expect(question_items).to_have_count(1)
        expect(question_items.first).to_contain_text("HIS-001")
        expect(question_items.first).to_contain_text("单选")
        expect(question_items.first).to_contain_text("洋务运动前期口号是？")
        expect(question_items.first).to_contain_text("答案 A · 2 分 · 洋务运动")

        qtype = page.get_by_label("题型")
        options = page.get_by_label("选项（每行一项，可选）")
        expect(options).to_be_visible()
        qtype.select_option("judge")
        expect(page.get_by_label("选项（每行一项，可选）")).to_have_count(0)
        qtype.select_option("fill")
        expect(page.get_by_label("选项（每行一项，可选）")).to_have_count(0)
        qtype.select_option("single")
        expect(page.get_by_label("选项（每行一项，可选）")).to_be_visible()
        qtype.select_option("multi")
        expect(page.get_by_label("选项（每行一项，可选）")).to_be_visible()

        save = page.get_by_role("button", name="保存题目")
        save.click()
        expect(page.locator(".error")).to_have_text("题干和标准答案不能为空")
        assert page.evaluate("window.__questionCalls.length") == 0

        page.get_by_label("题干").fill("只有题干")
        save.click()
        expect(page.locator(".error")).to_have_text("题干和标准答案不能为空")
        assert page.evaluate("window.__questionCalls.length") == 0

        page.get_by_label("题干").fill("")
        page.get_by_label("标准答案").fill("A")
        save.click()
        expect(page.locator(".error")).to_have_text("题干和标准答案不能为空")
        assert page.evaluate("window.__questionCalls.length") == 0

        page.get_by_label("题号（可选，需唯一）").fill(" his-002 ")
        page.get_by_label("题干").fill("  洋务运动的口号  ")
        page.get_by_label("标准答案").fill(" a, c ")
        page.get_by_label("分值").fill("4")
        page.get_by_label("主知识点").select_option("1")
        page.get_by_label("选项（每行一项，可选）").fill(
            "a. 自强\nB、求富\n\n无前缀选项"
        )
        save.click()

        expect(page.locator(".ok-banner")).to_have_text("题目已保存，可立即进入快速批改")
        expect(page.get_by_label("题号（可选，需唯一）")).to_have_value("")
        expect(page.get_by_label("题干")).to_have_value("")
        expect(page.get_by_label("标准答案")).to_have_value("")
        expect(page.get_by_label("选项（每行一项，可选）")).to_have_value("")
        expect(qtype).to_have_value("multi")
        expect(page.get_by_label("分值")).to_have_value("4")
        expect(page.get_by_label("主知识点")).to_have_value("1")
        expect(question_items).to_have_count(2)
        created_item = question_items.filter(has_text="his-002")
        expect(created_item).to_contain_text("多选")
        expect(created_item).to_contain_text("答案 a, c · 4 分 · 中国近代史")
        assert page.evaluate("window.__questionCalls") == [
            {"q": expected_question_input()}
        ]
        page.screenshot(path="/tmp/jiaofu-r2-question-tab-created.png", full_page=True)

        failed_page = browser.new_page(viewport={"width": 1440, "height": 1100})
        failed_page.add_init_script(QUESTION_MOCK_SCRIPT)
        reach_question_tab(failed_page, f"{base_url}?questionFail=1")
        failed_page.get_by_label("题型").select_option("fill")
        failed_page.get_by_label("题干").fill("辛亥革命的意义")
        failed_page.get_by_label("标准答案").fill("推翻清朝统治")
        failed_page.get_by_role("button", name="保存题目").click()

        expect(failed_page.locator(".error")).to_contain_text("模拟题目保存失败")
        expect(failed_page.get_by_label("题干")).to_have_value("辛亥革命的意义")
        expect(failed_page.get_by_label("标准答案")).to_have_value("推翻清朝统治")
        expect(failed_page.locator(".ok-banner")).to_have_count(0)
        expect(failed_page.locator(".question-item")).to_have_count(1)
        assert failed_page.evaluate("window.__questionCalls") == [{
            "q": {
                "subject_id": None,
                "question_no": None,
                "qtype": "fill",
                "stem": "辛亥革命的意义",
                "image_path": None,
                "correct_answer": "推翻清朝统治",
                "knowledge_point_id": None,
                "difficulty": None,
                "analysis": None,
                "max_score": 1,
                "enabled": True,
                "options": [],
            }
        }]
        failed_page.screenshot(path="/tmp/jiaofu-r2-question-tab-failed.png", full_page=True)
        browser.close()


if __name__ == "__main__":
    test_question_tab("http://127.0.0.1:4173")
    print("exam question tab UI characterization: PASS")
