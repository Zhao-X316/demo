"""固定答题卡主观题工作台工具栏的筛选、排序与统计行为。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_m25_rubric_update_ui import MOCK_SCRIPT as M25_BASE_MOCK_SCRIPT


TOOLBAR_MOCK_SCRIPT = M25_BASE_MOCK_SCRIPT + r"""
window.__subjectiveToolbarWriteCalls = [];
const __baseInvokeForSubjectiveToolbar = window.__TAURI_INTERNALS__.invoke;

function toolbarRow(overrides) {
  const base = {
    assessment_id: 1,
    assessment_version_id: 10,
    assessment_title: "第一单元练习",
    attempt_id: 1,
    attempt_state: "grading",
    active_publication_id: null,
    student_id: 1,
    student_no: "01",
    student_name: "甲同学",
    assessment_item_id: 101,
    order_index: 0,
    question_no: "1",
    question_type: "short_answer",
    question_stem: "概括洋务运动失败的根本原因",
    max_score: 4,
    answer_region_revision_id: 1001,
    crop_path: null,
    transcription_revision_id: 2001,
    transcription_revision: 1,
    result_state: "recognized",
    raw_ocr_text: "只学习技术，没有改变制度",
    normalized_text: "只学习技术，没有改变制度",
    teacher_corrected_text: null,
    confidence: 0.95,
    answer_json: JSON.stringify({ schema_version: 1, reference_answer: "没有改变封建制度" }),
    answer_slots_json: JSON.stringify({ schema_version: 1, answer_slots: [] }),
    rubric_points_json: JSON.stringify({
      schema_version: 1,
      rubric_points: [{
        source_public_id: "rubric-point-institution",
        stable_id: "institution",
        order_index: 0,
        canonical_text: "没有改变封建制度",
        max_score: 4,
      }],
    }),
    suggestion_id: 3001,
    short_answer_analysis_id: 4001,
    machine_grade_ai_run_id: 5001,
    suggestion_outcome: "partial",
    suggested_score: 2,
    suggestion_result_json: JSON.stringify({
      schema_version: 1,
      point_results: [{
        stable_id: "institution",
        status: "partial",
        suggested_score: 2,
        evidence_snippets: ["只学习技术"],
        reason: "表述不完整",
      }],
    }),
    batch_eligible: false,
    exclusion_reason: "TEACHER_REVIEW_REQUIRED",
    grade_decision_id: null,
    grade_decision_revision: null,
    teacher_score: null,
    confirmation_level: null,
    review_mode: null,
    current_suggestion_confirmed: false,
    decided_at: null,
    teacher_components_json: JSON.stringify({ schema_version: 1, component_results: [] }),
    accepted_answer_promotion_id: null,
    accepted_answer_promoted_at: null,
    rubric_evidence_promotions_json: JSON.stringify({ schema_version: 1, promotions: [] }),
  };
  const row = { ...base, ...overrides };
  if (row.question_type === "fill_blank") {
    row.answer_json = JSON.stringify({ schema_version: 1, accepted_answers: ["1842"] });
    row.answer_slots_json = JSON.stringify({
      schema_version: 1,
      answer_slots: [{
        source_public_id: `answer-slot-${row.assessment_item_id}`,
        stable_id: `slot-${row.assessment_item_id}`,
        order_index: 0,
        canonical_text: "1842",
        max_score: row.max_score,
      }],
    });
    row.rubric_points_json = JSON.stringify({ schema_version: 1, rubric_points: [] });
    row.short_answer_analysis_id = null;
    row.machine_grade_ai_run_id = null;
    row.suggestion_result_json = JSON.stringify({ schema_version: 1 });
  }
  if (row.current_suggestion_confirmed) {
    row.grade_decision_id = 7000 + row.suggestion_id;
    row.grade_decision_revision = 1;
    row.teacher_score = row.suggested_score ?? 0;
    row.confirmation_level = "teacher_accepted";
    row.review_mode = "teacher_accepted";
    row.decided_at = "2026-08-02T12:00:00Z";
  }
  return row;
}

function subjectiveToolbarWorkbench() {
  return {
    rows: [
      toolbarRow({
        student_id: 10, student_no: "10", student_name: "十号同学",
        answer_region_revision_id: 1010, transcription_revision_id: 2010,
        suggestion_id: 3010, current_suggestion_confirmed: true,
      }),
      toolbarRow({
        student_id: 2, student_no: "2", student_name: "二号同学",
        answer_region_revision_id: 1002, transcription_revision_id: 2002,
        suggestion_id: 3002, suggested_score: 1,
      }),
      toolbarRow({
        student_id: 1, student_no: "01", student_name: "一号同学",
        answer_region_revision_id: 1001, transcription_revision_id: 2001,
        suggestion_id: 3001, suggested_score: null, short_answer_analysis_id: null,
        machine_grade_ai_run_id: null, suggestion_outcome: null,
        suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
      }),
      toolbarRow({
        student_id: 3, student_no: "03", student_name: "三号同学",
        answer_region_revision_id: 1003, transcription_revision_id: 2003,
        suggestion_id: 3003, suggested_score: 2,
      }),
      toolbarRow({
        student_id: 11, student_no: "11", student_name: "十一号填空",
        assessment_item_id: 100, order_index: 2, question_no: "3",
        question_type: "fill_blank", question_stem: "《南京条约》签订于____年",
        max_score: 2, answer_region_revision_id: 1101,
        transcription_revision_id: 2101, suggestion_id: 3101,
        suggested_score: 2, current_suggestion_confirmed: true,
      }),
      toolbarRow({
        student_id: 12, student_no: "12", student_name: "十二号填空",
        assessment_item_id: 100, order_index: 2, question_no: "3",
        question_type: "fill_blank", question_stem: "《南京条约》签订于____年",
        max_score: 2, answer_region_revision_id: 1102,
        transcription_revision_id: 2102, suggestion_id: 3102,
        suggested_score: null, suggestion_outcome: null,
      }),
      toolbarRow({
        assessment_id: 2, assessment_version_id: 20, assessment_title: "第二单元测验",
        student_id: 21, student_no: "21", student_name: "二十一号第二题",
        assessment_item_id: 200, order_index: 4, question_no: "5",
        question_stem: "分析辛亥革命的历史意义",
        answer_region_revision_id: 1201, transcription_revision_id: 2201,
        suggestion_id: 3201, suggested_score: 2,
      }),
      toolbarRow({
        assessment_id: 2, assessment_version_id: 20, assessment_title: "第二单元测验",
        student_id: 22, student_no: "22", student_name: "二十二号第一题",
        assessment_item_id: 201, order_index: 1, question_no: "2",
        question_type: "fill_blank", question_stem: "辛亥革命爆发于____年",
        max_score: 2, answer_region_revision_id: 1202,
        transcription_revision_id: 2202, suggestion_id: 3202,
        suggested_score: null, suggestion_outcome: null,
      }),
    ],
    attempts: [],
  };
}

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (/correct|recognize|grade|accept|promote|save|replace|publish/.test(cmd)) {
    window.__subjectiveToolbarWriteCalls.push({ cmd, args: structuredClone(args) });
  }
  if (cmd === "exam_answer_sheet_subjective_workbench") {
    return subjectiveToolbarWorkbench();
  }
  if (cmd === "exam_subjective_link_editor") {
    return {
      assessment_item_id: args.assessmentItemId,
      question_version_id: args.assessmentItemId,
      question_type: "short_answer",
      question_stem: "工具栏特征测试",
      current_assessment_version_id: 1,
      current_assessment_revision: 1,
      current_link_set_id: 1,
      current_link_set_revision: 1,
      knowledge_map_public_id: "map-1",
      knowledge_map_title: "中国历史八上",
      sources: [],
      knowledge_options: [],
      ability_options: [],
    };
  }
  return __baseInvokeForSubjectiveToolbar(cmd, args);
};
"""


def open_subjective_review(page: Page, url: str) -> None:
    page.add_init_script(TOOLBAR_MOCK_SCRIPT)
    page.goto(url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    page.get_by_role("button", name="答题卡主观题").click()
    expect(page.locator("section.objective-toolbar")).to_be_visible()


def stat_texts(page: Page) -> list[str]:
    return page.locator("section.objective-toolbar .objective-stats span").all_text_contents()


def student_names(page: Page) -> list[str]:
    return page.locator(".objective-review-list .objective-student b").all_text_contents()


def test_toolbar_filtering_and_stats(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1100})
        open_subjective_review(page, base_url)

        toolbar = page.locator("section.objective-toolbar")
        version_select = toolbar.locator("label.field").filter(has_text="作业版本").locator("select")
        item_select = toolbar.locator("label.field").filter(has_text="按题终审").locator("select")
        expect(version_select).to_have_value("10")
        expect(item_select).to_have_value("101")
        assert item_select.locator("option").evaluate_all(
            "options => options.map(option => option.value)"
        ) == ["101", "100"]
        expect(item_select.locator("option:checked")).to_have_text(
            "第1题 · 简答 · 概括洋务运动失败的根本原因"
        )
        assert stat_texts(page) == [
            "4 份作答",
            "1 已确认",
            "2 有明确建议",
            "1 需人工记分",
        ]
        assert student_names(page) == ["一号同学", "二号同学", "三号同学", "十号同学"]
        expect(
            page.get_by_text(
                "填空题只做已确认答案的精确匹配；简答题逐点引用学生原文给建议，不按整段相似度直接给分，也不自动确认。",
                exact=True,
            )
        ).to_be_visible()

        item_select.select_option("100")
        expect(item_select).to_have_value("100")
        assert stat_texts(page) == [
            "2 份作答",
            "1 已确认",
            "0 有明确建议",
            "1 需人工记分",
        ]
        assert student_names(page) == ["十一号填空", "十二号填空"]
        page.screenshot(path="/tmp/jiaofu-r2-subjective-toolbar-item-switch.png", full_page=True)

        version_select.select_option("20")
        expect(version_select).to_have_value("20")
        expect(item_select).to_have_value("201")
        assert item_select.locator("option").evaluate_all(
            "options => options.map(option => option.value)"
        ) == ["201", "200"]
        expect(item_select.locator("option:checked")).to_have_text(
            "第2题 · 填空 · 辛亥革命爆发于____年"
        )
        assert stat_texts(page) == [
            "1 份作答",
            "0 已确认",
            "0 有明确建议",
            "1 需人工记分",
        ]
        assert student_names(page) == ["二十二号第一题"]
        assert page.evaluate("window.__subjectiveToolbarWriteCalls") == []
        page.screenshot(path="/tmp/jiaofu-r2-subjective-toolbar-version-switch.png", full_page=True)
        browser.close()


if __name__ == "__main__":
    test_toolbar_filtering_and_stats("http://127.0.0.1:4173/")
    print("exam subjective review toolbar UI characterization: PASS")
