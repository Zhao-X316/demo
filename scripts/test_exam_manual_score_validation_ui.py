"""人工评分空白必须阻断 IPC，显式零分仍可保存；复用合成 Tauri 夹具。"""

import os
import unittest

from playwright.sync_api import expect, sync_playwright

from test_exam_objective_review_tab_ui import (
    OBJECTIVE_MOCK_SCRIPT,
    objective_row,
    reach_objective_tab,
)
from test_exam_dictation_review_tab_ui import (
    DICTATION_MOCK_SCRIPT,
    dictation_row,
    reach_dictation_tab,
)
import test_exam_subjective_review_tab_ui as subjective
import test_exam_subjective_component_editor_ui as components


BASE_URL = os.environ.get("JIAOFU_TEST_BASE_URL", "http://127.0.0.1:4173")


class ManualScoreValidationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        cls.browser = cls.playwright.chromium.launch(headless=True)

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def new_page(self):
        page = self.browser.new_page(viewport={"width": 1440, "height": 1200})
        self.addCleanup(page.close)
        return page

    def assert_blocked(self, page, save, calls_expression):
        save.click()
        self.assertEqual(page.evaluate(calls_expression), [], "空白评分不应发送终审请求")
        expect(page.locator(".error")).to_be_visible()
        self.assertIn("得分", page.locator(".error").inner_text())

    def test_objective_missing_or_cleared_score_requires_input_but_accepts_zero(self):
        for name in ["十一号识别失败", "十号高置信"]:
            with self.subTest(student=name):
                page = self.new_page()
                page.add_init_script(OBJECTIVE_MOCK_SCRIPT)
                reach_objective_tab(page, BASE_URL)
                row = objective_row(page, name)
                row.get_by_text("人工记分", exact=True).click()
                score = row.get_by_label("得分（满分 2）")
                if name == "十号高置信":
                    expect(score).to_have_value("2")
                    score.fill("")
                expect(score).to_have_value("")
                row.get_by_label("证据依据").fill("已按原图复核")
                save = row.get_by_role("button", name="保存人工 revision")
                self.assert_blocked(page, save, "window.__objectiveCalls")
                score.fill("0")
                save.click()
                expect(page.locator(".ok-banner")).to_be_visible()
                calls = page.evaluate("window.__objectiveCalls")
                self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0]["cmd"], "exam_objective_correct")
                self.assertEqual(calls[0]["args"]["teacherScore"], 0)

    def test_dictation_missing_or_cleared_score_requires_input_but_accepts_zero(self):
        for name in ["十一号识别失败", "十号精确"]:
            with self.subTest(student=name):
                page = self.new_page()
                page.add_init_script(DICTATION_MOCK_SCRIPT)
                reach_dictation_tab(page, BASE_URL)
                row = dictation_row(page, name)
                row.get_by_text("人工记分 / 补录", exact=True).click()
                score = row.get_by_label("得分（满分 2）")
                if name == "十号精确":
                    expect(score).to_have_value("2")
                    score.fill("")
                expect(score).to_have_value("")
                row.get_by_label("判定依据（必填）").fill("已按原图复核")
                save = row.get_by_role("button", name="保存人工 revision")
                self.assert_blocked(page, save, "window.__dictationCalls")
                score.fill("0")
                save.click()
                expect(page.locator(".ok-banner")).to_be_visible()
                calls = page.evaluate("window.__dictationCalls")
                self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0]["cmd"], "exam_dictation_correct_grade")
                self.assertEqual(calls[0]["args"]["teacherScore"], 0)

    def test_subjective_whole_score_requires_input_but_accepts_zero(self):
        page = self.new_page()
        subjective.open_subjective_review(page, BASE_URL)
        editor = subjective.whole_score_editor(subjective.student_card(page, "十一号总分"))
        score = editor.locator("label.field").filter(has_text="得分").locator("input")
        expect(score).to_have_value("")
        editor.locator("label.field").filter(has_text="判定依据").locator("input").fill("已按原图复核")
        save = editor.get_by_role("button", name="保存人工 revision")
        self.assert_blocked(page, save, "window.__subjectiveTabCalls")
        score.fill("0")
        save.click()
        expect(page.locator(".ok-banner")).to_be_visible()
        self.assertEqual(subjective.write_calls(page), [{
            "cmd": "exam_answer_sheet_subjective_correct",
            "args": {"suggestionId": 3011, "teacherScore": 0, "teacherNote": "已按原图复核"},
        }])

    def test_each_component_requires_input_before_any_score_is_saved(self):
        page = self.new_page()
        components.open_subjective_review(page, BASE_URL)
        components.item_select(page).select_option("403")
        editor = components.component_editor(components.student_card(page, "多空同学"), "按空格逐项确认")
        scores = editor.locator('.component-editor input[type="number"]')
        self.assertEqual(scores.count(), 2)
        for score in scores.all():
            expect(score).to_have_value("")
        components.field_input(editor, "本题整体判定依据").fill("已按原图逐空核对")
        save = editor.get_by_role("button", name="保存逐项结论并自动汇总")
        calls_expression = "window.__subjectiveComponentEditorWriteCalls"
        self.assert_blocked(page, save, calls_expression)
        scores.nth(0).fill("0")
        self.assert_blocked(page, save, calls_expression)
        scores.nth(1).fill("0")
        save.click()
        expect(page.locator(".ok-banner")).to_be_visible()
        calls = page.evaluate(calls_expression)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0]["cmd"], "exam_answer_sheet_subjective_correct_components")
        self.assertEqual(calls[0]["args"]["components"], [
            {"source_type": "answer_slot", "source_public_id": source,
             "teacher_score": 0, "evidence_text": None, "teacher_note": None}
            for source in ["slot-year", "slot-port"]
        ])


if __name__ == "__main__":
    unittest.main(verbosity=2)
