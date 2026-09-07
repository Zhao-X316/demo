"""固定答题卡主观题证据摘要的状态、事实与逐点评分展示。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_exam_subjective_review_toolbar_ui import (
    TOOLBAR_MOCK_SCRIPT as TOOLBAR_BASE_MOCK_SCRIPT,
)


EVIDENCE_MOCK_SCRIPT = TOOLBAR_BASE_MOCK_SCRIPT + r"""
window.__subjectiveEvidenceWriteCalls = [];
const __baseInvokeForSubjectiveEvidence = window.__TAURI_INTERNALS__.invoke;

function evidenceRow(overrides, finalOverrides = {}) {
  return Object.assign(toolbarRow(overrides), finalOverrides);
}

function subjectiveEvidenceWorkbench() {
  const rubric = JSON.stringify({
    schema_version: 1,
    rubric_points: [
      {
        source_public_id: "rubric-system",
        stable_id: "system",
        order_index: 0,
        canonical_text: "只学习西方技术，没有改变封建制度",
        max_score: 3,
      },
      {
        source_public_id: "rubric-support",
        stable_id: "support",
        order_index: 1,
        canonical_text: "缺乏广泛社会基础",
        max_score: 3,
      },
    ],
  });
  const analysis = JSON.stringify({
    schema_version: 1,
    point_results: [
      {
        stable_id: "system",
        status: "covered",
        suggested_score: 3,
        evidence_snippets: ["只学习技术"],
        reason: "学生明确指出制度局限",
      },
      {
        stable_id: "support",
        status: "missing",
        suggested_score: 0,
        evidence_snippets: [],
        reason: "",
      },
    ],
  });
  const promotions = JSON.stringify({
    schema_version: 1,
    promotions: [
      {
        promotion_id: 901,
        source_public_id: "rubric-system",
        rubric_point_stable_id: "system",
        evidence_text: "只学习技术",
        adopted_assessment_version_id: 31,
        adopted_rubric_version_id: 2,
        adopted_link_set_id: 2,
        created_at: "2026-08-02T12:00:00Z",
      },
      {
        promotion_id: 902,
        source_public_id: "rubric-support",
        rubric_point_stable_id: "support",
        evidence_text: "缺少群众基础",
        adopted_assessment_version_id: 32,
        adopted_rubric_version_id: 3,
        adopted_link_set_id: 3,
        created_at: "2026-08-02T12:01:00Z",
      },
    ],
  });

  return {
    rows: [
      evidenceRow({
        assessment_id: 3,
        assessment_version_id: 30,
        assessment_title: "证据展示测试",
        student_id: 31,
        student_no: "01",
        student_name: "已终审同学",
        assessment_item_id: 301,
        order_index: 0,
        question_no: "1",
        question_type: "short_answer",
        question_stem: "分析洋务运动失败的原因",
        max_score: 6,
        answer_region_revision_id: 1301,
        crop_path: "/tmp/confirmed-answer.png",
        transcription_revision_id: 2301,
        raw_ocr_text: "只学习技术，缺少群众基础",
        normalized_text: "只学习技术，缺少群众基础",
        teacher_corrected_text: "只学习技术，没有改变制度",
        confidence: 0.876,
        answer_json: JSON.stringify({ reference_answer: "制度局限与社会基础" }),
        rubric_points_json: rubric,
        suggestion_id: 3301,
        short_answer_analysis_id: 4301,
        machine_grade_ai_run_id: 5301,
        suggestion_outcome: "partial",
        suggested_score: 3,
        suggestion_result_json: analysis,
        current_suggestion_confirmed: true,
        rubric_evidence_promotions_json: promotions,
      }, {
        grade_decision_id: 7301,
        teacher_score: 4,
        confirmation_level: "teacher_corrected",
        review_mode: "teacher_corrected",
      }),
      evidenceRow({
        assessment_id: 3,
        assessment_version_id: 30,
        assessment_title: "证据展示测试",
        student_id: 32,
        student_no: "02",
        student_name: "识别失败同学",
        assessment_item_id: 301,
        order_index: 0,
        question_no: "1",
        question_type: "short_answer",
        question_stem: "分析洋务运动失败的原因",
        max_score: 6,
        answer_region_revision_id: 1302,
        crop_path: null,
        transcription_revision_id: 2302,
        result_state: "recognize_failed",
        raw_ocr_text: null,
        normalized_text: null,
        confidence: null,
        answer_json: JSON.stringify({ reference_answer: "制度局限与社会基础" }),
        rubric_points_json: rubric,
        suggestion_id: 3302,
        short_answer_analysis_id: null,
        machine_grade_ai_run_id: null,
        suggestion_outcome: "unscored",
        suggested_score: null,
        suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
      }),
      evidenceRow({
        assessment_id: 3,
        assessment_version_id: 30,
        assessment_title: "证据展示测试",
        student_id: 33,
        student_no: "03",
        student_name: "评分点缺失同学",
        assessment_item_id: 302,
        order_index: 1,
        question_no: "2",
        question_type: "short_answer",
        question_stem: "评价辛亥革命",
        max_score: 4,
        answer_region_revision_id: 1303,
        transcription_revision_id: 2303,
        result_state: "unreadable",
        raw_ocr_text: "字迹模糊",
        confidence: 0.32,
        answer_json: JSON.stringify({ reference_answer: "历史意义与局限" }),
        rubric_points_json: JSON.stringify({ schema_version: 1, rubric_points: [] }),
        suggestion_id: 3303,
        short_answer_analysis_id: null,
        machine_grade_ai_run_id: null,
        suggestion_outcome: "unscored",
        suggested_score: null,
        suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
      }),
      evidenceRow({
        assessment_id: 3,
        assessment_version_id: 30,
        assessment_title: "证据展示测试",
        student_id: 34,
        student_no: "04",
        student_name: "填空改判同学",
        assessment_item_id: 303,
        order_index: 2,
        question_no: "3",
        question_type: "fill_blank",
        question_stem: "《南京条约》签订于____年",
        max_score: 2,
        answer_region_revision_id: 1304,
        transcription_revision_id: 2304,
        raw_ocr_text: null,
        normalized_text: "一八四二年",
        teacher_corrected_text: "一八四二年",
        confidence: null,
        suggestion_id: 3304,
        suggestion_outcome: "incorrect",
        suggested_score: 0,
        current_suggestion_confirmed: true,
        accepted_answer_promotion_id: 9901,
      }, {
        grade_decision_id: 7304,
        teacher_score: 2,
        confirmation_level: "teacher_corrected",
        review_mode: "teacher_corrected",
        answer_json: JSON.stringify({ canonical_answers: ["1842年"] }),
      }),
    ],
    attempts: [],
  };
}

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (/correct|recognize|grade|accept|promote|save|replace|publish/.test(cmd)) {
    window.__subjectiveEvidenceWriteCalls.push({ cmd, args: structuredClone(args) });
  }
  if (cmd === "exam_answer_sheet_subjective_workbench") {
    return subjectiveEvidenceWorkbench();
  }
  return __baseInvokeForSubjectiveEvidence(cmd, args);
};
"""


def open_subjective_review(page: Page, url: str) -> None:
    page.add_init_script(EVIDENCE_MOCK_SCRIPT)
    page.goto(url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    page.get_by_role("button", name="答题卡主观题").click()
    expect(page.locator(".objective-review-list")).to_be_visible()


def student_card(page: Page, student_name: str):
    return page.locator("article.objective-review-row").filter(has_text=student_name)


def item_select(page: Page):
    toolbar = page.locator("section.objective-toolbar")
    return toolbar.locator("label.field").filter(has_text="按题终审").locator("select")


def test_evidence_summary_states(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1200})
        open_subjective_review(page, base_url)

        confirmed = student_card(page, "已终审同学")
        expect(confirmed).to_have_class("objective-review-row confirmed")
        expect(confirmed.get_by_text("01号 · 第1题", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("简答 · 满分 6", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("已终审", exact=True).first).to_be_visible()
        image = confirmed.get_by_role("img", name="已终审同学 第1题作答")
        expect(image).to_be_visible()
        assert image.get_attribute("src").endswith("/tmp/confirmed-answer.png")
        expect(confirmed.get_by_text("逐点评分建议：部分覆盖", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("只学习技术，缺少群众基础", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("只学习技术，没有改变制度", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("制度局限与社会基础", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("88%", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("3 / 6", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("4 分 · 人工修正", exact=True)).to_be_visible()
        expect(confirmed.get_by_text("已沉淀 2 条老师确认表述", exact=True)).to_be_visible()

        covered = confirmed.locator(".short-answer-point.covered")
        expect(covered.get_by_text("只学习西方技术，没有改变封建制度", exact=True)).to_be_visible()
        expect(covered.get_by_text("已覆盖 · 3 / 3 分", exact=True)).to_be_visible()
        expect(covered.get_by_text("学生明确指出制度局限", exact=True)).to_be_visible()
        expect(covered.locator("q")).to_have_text("只学习技术")
        missing = confirmed.locator(".short-answer-point.missing")
        expect(missing.get_by_text("未覆盖 · 0 / 3 分", exact=True)).to_be_visible()
        expect(missing.get_by_text("等待老师结合原图核对", exact=True)).to_be_visible()
        expect(missing.get_by_text("学生答案中未定位到可引用片段", exact=True)).to_be_visible()

        failed = student_card(page, "识别失败同学")
        expect(failed.get_by_text("需老师处理", exact=True)).to_be_visible()
        expect(failed.get_by_text("裁剪图不可用", exact=True)).to_be_visible()
        expect(failed.get_by_text("识别失败", exact=True).first).to_be_visible()
        expect(failed.locator(".objective-facts").get_by_text("—", exact=True)).to_have_count(3)
        expect(failed.get_by_text("只学习西方技术，没有改变封建制度（3分）", exact=True)).to_be_visible()
        expect(failed.get_by_text("缺乏广泛社会基础（3分）", exact=True)).to_be_visible()
        page.screenshot(path="/tmp/jiaofu-r2-subjective-evidence-analysis.png", full_page=True)

        item_select(page).select_option("302")
        unreadable = student_card(page, "评分点缺失同学")
        expect(unreadable.get_by_text("无法辨认", exact=True).first).to_be_visible()
        expect(unreadable.get_by_text("32%", exact=True)).to_be_visible()
        expect(unreadable.get_by_text("评分点尚未完整，必须老师人工核对", exact=True)).to_be_visible()

        item_select(page).select_option("303")
        fill = student_card(page, "填空改判同学")
        expect(fill.get_by_text("填空 · 满分 2", exact=True)).to_be_visible()
        expect(fill.get_by_text("与已确认答案不一致", exact=True)).to_be_visible()
        expect(fill.get_by_text("1842年", exact=True)).to_be_visible()
        expect(fill.get_by_text("0 / 2", exact=True)).to_be_visible()
        expect(fill.get_by_text("2 分 · 人工修正", exact=True)).to_be_visible()
        expect(fill.get_by_text("已加入未来可接受写法", exact=True)).to_be_visible()
        assert page.evaluate("window.__subjectiveEvidenceWriteCalls") == []
        page.screenshot(path="/tmp/jiaofu-r2-subjective-evidence-fallbacks.png", full_page=True)
        browser.close()


if __name__ == "__main__":
    test_evidence_summary_states("http://127.0.0.1:4173/")
    print("exam subjective evidence summary UI characterization: PASS")
