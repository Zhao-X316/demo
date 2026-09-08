from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""固定答题卡主观题逐项确认编辑器的默认值、校验与提交 DTO。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_exam_subjective_evidence_summary_ui import EVIDENCE_MOCK_SCRIPT


COMPONENT_EDITOR_MOCK_SCRIPT = EVIDENCE_MOCK_SCRIPT + r"""
window.__subjectiveComponentEditorCalls = [];
window.__subjectiveComponentEditorWriteCalls = [];
window.__subjectiveComponentEditorConfirmed = {};
const __baseInvokeForSubjectiveComponentEditor = window.__TAURI_INTERNALS__.invoke;

function componentEditorRow(overrides, finalOverrides = {}) {
  return evidenceRow(overrides, finalOverrides);
}

function componentEditorWorkbench() {
  const shortRubric = JSON.stringify({
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
  const shortAnalysis = JSON.stringify({
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
        reason: "学生没有写出群众基础",
      },
    ],
  });
  const multiSlots = JSON.stringify({
    schema_version: 1,
    answer_slots: [
      {
        source_public_id: "slot-year",
        stable_id: "year",
        order_index: 0,
        canonical_answers_json: JSON.stringify({ answers: ["1842年"] }),
        max_score: 1,
      },
      {
        source_public_id: "slot-port",
        stable_id: "port",
        order_index: 1,
        canonical_answers_json: JSON.stringify({ answers: ["广州"] }),
        max_score: 1,
      },
    ],
  });
  const singleSlot = JSON.stringify({
    schema_version: 1,
    answer_slots: [
      {
        source_public_id: "slot-single",
        stable_id: "single",
        order_index: 0,
        canonical_answers_json: JSON.stringify({ answers: ["1842年"] }),
        max_score: 2,
      },
    ],
  });

  const rows = [
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 41,
      student_no: "01",
      student_name: "简答默认同学",
      assessment_item_id: 401,
      order_index: 0,
      question_no: "1",
      question_type: "short_answer",
      question_stem: "分析洋务运动失败的原因",
      max_score: 6,
      answer_region_revision_id: 1401,
      transcription_revision_id: 2401,
      raw_ocr_text: "只学习技术",
      normalized_text: "只学习技术",
      answer_json: JSON.stringify({ reference_answer: "制度局限与群众基础" }),
      rubric_points_json: shortRubric,
      suggestion_id: 4401,
      short_answer_analysis_id: 5401,
      machine_grade_ai_run_id: 6401,
      suggestion_outcome: "partial",
      suggested_score: 3,
      suggestion_result_json: shortAnalysis,
    }),
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 42,
      student_no: "02",
      student_name: "已确认同学",
      assessment_item_id: 401,
      order_index: 0,
      question_no: "1",
      question_type: "short_answer",
      question_stem: "分析洋务运动失败的原因",
      max_score: 6,
      answer_region_revision_id: 1402,
      transcription_revision_id: 2402,
      rubric_points_json: shortRubric,
      suggestion_id: 4402,
      short_answer_analysis_id: 5402,
      machine_grade_ai_run_id: 6402,
      suggestion_outcome: "partial",
      suggested_score: 3,
      suggestion_result_json: shortAnalysis,
      current_suggestion_confirmed: true,
    }),
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 43,
      student_no: "03",
      student_name: "无评分点同学",
      assessment_item_id: 402,
      order_index: 1,
      question_no: "2",
      question_type: "short_answer",
      question_stem: "评价辛亥革命",
      max_score: 4,
      answer_region_revision_id: 1403,
      transcription_revision_id: 2403,
      rubric_points_json: JSON.stringify({ schema_version: 1, rubric_points: [] }),
      suggestion_id: 4403,
      short_answer_analysis_id: null,
      machine_grade_ai_run_id: null,
      suggestion_outcome: "unscored",
      suggested_score: null,
      suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
    }),
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 44,
      student_no: "04",
      student_name: "多空同学",
      assessment_item_id: 403,
      order_index: 2,
      question_no: "3",
      question_type: "fill_blank",
      question_stem: "《南京条约》签订于____年，开放____等通商口岸",
      max_score: 2,
      answer_region_revision_id: 1404,
      transcription_revision_id: 2404,
      raw_ocr_text: "1842年 广州",
      normalized_text: "1842年；广州",
      suggestion_id: 4404,
      suggestion_outcome: "partial",
      suggested_score: 1,
    }, {
      answer_slots_json: multiSlots,
    }),
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 45,
      student_no: "05",
      student_name: "单空老师校正",
      assessment_item_id: 404,
      order_index: 3,
      question_no: "4",
      question_type: "fill_blank",
      question_stem: "《南京条约》签订于____年",
      max_score: 2,
      answer_region_revision_id: 1405,
      transcription_revision_id: 2405,
      raw_ocr_text: "一八四二",
      normalized_text: "1842",
      teacher_corrected_text: "一八四二年",
      suggestion_id: 4405,
      suggestion_outcome: "correct",
      suggested_score: 2,
    }, {
      answer_slots_json: singleSlot,
    }),
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 46,
      student_no: "06",
      student_name: "单空规范化",
      assessment_item_id: 404,
      order_index: 3,
      question_no: "4",
      question_type: "fill_blank",
      question_stem: "《南京条约》签订于____年",
      max_score: 2,
      answer_region_revision_id: 1406,
      transcription_revision_id: 2406,
      raw_ocr_text: "一八四二",
      normalized_text: "1842年",
      teacher_corrected_text: null,
      suggestion_id: 4406,
      suggestion_outcome: "correct",
      suggested_score: 2,
    }, {
      answer_slots_json: singleSlot,
    }),
    componentEditorRow({
      assessment_id: 4,
      assessment_version_id: 40,
      assessment_title: "逐项确认测试",
      student_id: 47,
      student_no: "07",
      student_name: "单空原始",
      assessment_item_id: 404,
      order_index: 3,
      question_no: "4",
      question_type: "fill_blank",
      question_stem: "《南京条约》签订于____年",
      max_score: 2,
      answer_region_revision_id: 1407,
      transcription_revision_id: 2407,
      raw_ocr_text: "一八四二年",
      normalized_text: null,
      teacher_corrected_text: null,
      suggestion_id: 4407,
      suggestion_outcome: "correct",
      suggested_score: 2,
    }, {
      answer_slots_json: singleSlot,
    }),
  ];

  return {
    rows: rows.map((row) => {
      const confirmed = Boolean(window.__subjectiveComponentEditorConfirmed[row.suggestion_id]);
      if (!confirmed) return row;
      return {
        ...row,
        current_suggestion_confirmed: true,
        grade_decision_id: 8000 + row.suggestion_id,
        grade_decision_revision: 1,
        teacher_score: window.__subjectiveComponentEditorConfirmed[row.suggestion_id],
        confirmation_level: "teacher_corrected",
        review_mode: "teacher_corrected",
        decided_at: "2026-08-02T14:00:00Z",
      };
    }),
    attempts: [],
  };
}

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (/correct|recognize|grade|accept|promote|save|replace|publish/.test(cmd)) {
    window.__subjectiveComponentEditorWriteCalls.push({ cmd, args: structuredClone(args) });
  }
  if (cmd === "exam_answer_sheet_subjective_workbench") {
    return componentEditorWorkbench();
  }
  if (cmd === "exam_answer_sheet_subjective_correct_components") {
    const call = { cmd, args: structuredClone(args) };
    window.__subjectiveComponentEditorCalls.push(call);
    const teacherScore = args.components.reduce(
      (total, component) => total + component.teacher_score,
      0,
    );
    window.__subjectiveComponentEditorConfirmed[args.suggestionId] = teacherScore;
    return { teacher_score: teacherScore };
  }
  return __baseInvokeForSubjectiveComponentEditor(cmd, args);
};
"""


def open_subjective_review(page: Page, url: str) -> None:
    page.add_init_script(COMPONENT_EDITOR_MOCK_SCRIPT)
    page.goto(url)
    page.wait_for_load_state("networkidle")
    navigate_exam(page)
    page.get_by_role("button", name="答题卡主观题").click()
    expect(page.locator(".objective-review-list")).to_be_visible()


def student_card(page: Page, student_name: str):
    return page.locator("article.objective-review-row").filter(has_text=student_name)


def item_select(page: Page):
    toolbar = page.locator("section.objective-toolbar")
    return toolbar.locator("label.field").filter(has_text="按题终审").locator("select")


def component_editor(card, summary: str):
    return card.locator("details").filter(has_text=summary).first


def component_point(editor, label: str):
    return editor.locator(".component-editor .short-answer-point").filter(has_text=label)


def field_input(container, label: str):
    return container.locator("label.field").filter(has_text=label).locator("input")


def test_subjective_component_editor(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1250})
        open_subjective_review(page, base_url)

        short_card = student_card(page, "简答默认同学")
        short_editor = component_editor(short_card, "按评分点逐项确认")
        expect(short_editor).to_be_visible()
        assert short_editor.evaluate("node => node.open") is True
        expect(short_editor.locator("summary")).to_have_text("按评分点逐项确认")

        system_point = component_point(short_editor, "只学习西方技术，没有改变封建制度")
        support_point = component_point(short_editor, "缺乏广泛社会基础")
        expect(field_input(system_point, "本项得分")).to_have_value("3")
        expect(field_input(system_point, "学生作答证据")).to_have_value("只学习技术")
        expect(field_input(system_point, "本项备注")).to_have_value("")
        expect(field_input(support_point, "本项得分")).to_have_value("0")
        expect(field_input(support_point, "学生作答证据")).to_have_value("")
        expect(field_input(support_point, "本项备注")).to_have_value("")

        confirmed_card = student_card(page, "已确认同学")
        expect(confirmed_card.locator("summary").filter(has_text="按评分点逐项确认")).to_have_count(0)

        save_short = short_editor.get_by_role("button", name="保存逐项结论并自动汇总")
        save_short.click()
        expect(page.locator(".error")).to_have_text("逐项终审仍需填写本题整体判定依据")
        assert page.evaluate("window.__subjectiveComponentEditorCalls") == []

        overall_note = field_input(short_editor, "本题整体判定依据")
        overall_note.fill("制度局限正确，群众基础部分覆盖")
        system_score = field_input(system_point, "本项得分")
        system_evidence = field_input(system_point, "学生作答证据")
        system_score.fill("4")
        save_short.click()
        expect(page.locator(".error")).to_have_text(
            "只学习西方技术，没有改变封建制度：得分必须位于 0~3，给分时必须填写学生作答证据"
        )
        assert page.evaluate("window.__subjectiveComponentEditorCalls") == []

        system_score.fill("3")
        system_evidence.fill("")
        save_short.click()
        expect(page.locator(".error")).to_have_text(
            "只学习西方技术，没有改变封建制度：得分必须位于 0~3，给分时必须填写学生作答证据"
        )
        assert page.evaluate("window.__subjectiveComponentEditorCalls") == []

        system_evidence.fill("只学习技术")
        field_input(system_point, "本项备注").fill("准确命中")
        field_input(support_point, "本项得分").fill("1")
        field_input(support_point, "学生作答证据").fill("没有群众基础")
        field_input(support_point, "本项备注").fill("表述不完整")
        page.screenshot(
            path="/tmp/jiaofu-r2-subjective-component-editor-short-answer.png",
            full_page=True,
        )
        save_short.click()
        expect(page.locator(".ok-banner")).to_have_text(
            "已逐项确认 简答默认同学 的第1题，自动汇总为 4 分"
        )
        expect(short_card.locator("summary").filter(has_text="按评分点逐项确认")).to_have_count(0)
        assert page.evaluate("window.__subjectiveComponentEditorCalls") == [
            {
                "cmd": "exam_answer_sheet_subjective_correct_components",
                "args": {
                    "suggestionId": 4401,
                    "components": [
                        {
                            "source_type": "rubric_point",
                            "source_public_id": "rubric-system",
                            "teacher_score": 3,
                            "evidence_text": "只学习技术",
                            "teacher_note": "准确命中",
                        },
                        {
                            "source_type": "rubric_point",
                            "source_public_id": "rubric-support",
                            "teacher_score": 1,
                            "evidence_text": "没有群众基础",
                            "teacher_note": "表述不完整",
                        },
                    ],
                    "teacherNote": "制度局限正确，群众基础部分覆盖",
                },
            }
        ]

        item_select(page).select_option("402")
        no_components = student_card(page, "无评分点同学")
        expect(no_components.locator("summary").filter(has_text="按评分点逐项确认")).to_have_count(0)

        item_select(page).select_option("403")
        multi_card = student_card(page, "多空同学")
        multi_editor = component_editor(multi_card, "按空格逐项确认")
        expect(multi_editor).to_be_visible()
        assert multi_editor.evaluate("node => node.open") is True
        year_point = component_point(multi_editor, "1842年")
        port_point = component_point(multi_editor, "广州")
        expect(field_input(year_point, "本项得分")).to_have_value("")
        expect(field_input(year_point, "学生作答证据")).to_have_value("")
        expect(field_input(port_point, "本项得分")).to_have_value("")
        expect(field_input(port_point, "学生作答证据")).to_have_value("")
        field_input(year_point, "本项得分").fill("1")
        field_input(year_point, "学生作答证据").fill("1842年")
        field_input(year_point, "本项备注").fill("正确")
        field_input(port_point, "本项得分").fill("0")
        field_input(port_point, "本项备注").fill("未作答")
        field_input(multi_editor, "本题整体判定依据").fill("第1空正确，第2空未作答")
        page.screenshot(
            path="/tmp/jiaofu-r2-subjective-component-editor-multi-fill.png",
            full_page=True,
        )
        multi_editor.get_by_role("button", name="保存逐项结论并自动汇总").click()
        expect(page.locator(".ok-banner")).to_have_text(
            "已逐项确认 多空同学 的第3题，自动汇总为 1 分"
        )
        assert page.evaluate("window.__subjectiveComponentEditorCalls") == [
            {
                "cmd": "exam_answer_sheet_subjective_correct_components",
                "args": {
                    "suggestionId": 4401,
                    "components": [
                        {
                            "source_type": "rubric_point",
                            "source_public_id": "rubric-system",
                            "teacher_score": 3,
                            "evidence_text": "只学习技术",
                            "teacher_note": "准确命中",
                        },
                        {
                            "source_type": "rubric_point",
                            "source_public_id": "rubric-support",
                            "teacher_score": 1,
                            "evidence_text": "没有群众基础",
                            "teacher_note": "表述不完整",
                        },
                    ],
                    "teacherNote": "制度局限正确，群众基础部分覆盖",
                },
            },
            {
                "cmd": "exam_answer_sheet_subjective_correct_components",
                "args": {
                    "suggestionId": 4404,
                    "components": [
                        {
                            "source_type": "answer_slot",
                            "source_public_id": "slot-year",
                            "teacher_score": 1,
                            "evidence_text": "1842年",
                            "teacher_note": "正确",
                        },
                        {
                            "source_type": "answer_slot",
                            "source_public_id": "slot-port",
                            "teacher_score": 0,
                            "evidence_text": None,
                            "teacher_note": "未作答",
                        },
                    ],
                    "teacherNote": "第1空正确，第2空未作答",
                },
            },
        ]

        item_select(page).select_option("404")
        fallback_expectations = {
            "单空老师校正": "一八四二年",
            "单空规范化": "1842年",
            "单空原始": "一八四二年",
        }
        for student_name, expected_evidence in fallback_expectations.items():
            card = student_card(page, student_name)
            editor = component_editor(card, "按空格逐项确认")
            assert editor.evaluate("node => node.open") is False
            point = component_point(editor, "1842年")
            expect(field_input(point, "本项得分")).to_have_value("2")
            expect(field_input(point, "学生作答证据")).to_have_value(expected_evidence)

        writes = page.evaluate("window.__subjectiveComponentEditorWriteCalls")
        assert [call["cmd"] for call in writes] == [
            "exam_answer_sheet_subjective_correct_components",
            "exam_answer_sheet_subjective_correct_components",
        ]
        browser.close()


if __name__ == "__main__":
    test_subjective_component_editor("http://127.0.0.1:4173/")
    print("exam subjective component editor UI characterization: PASS")
