"""固定完整答题卡主观题终审 Tab 的命令串联、门禁与失败保留。"""

import re
from typing import Optional

from playwright.sync_api import Page, expect, sync_playwright

from test_exam_subjective_review_toolbar_ui import (
    TOOLBAR_MOCK_SCRIPT as TOOLBAR_BASE_MOCK_SCRIPT,
)


SUBJECTIVE_TAB_MOCK_SCRIPT = TOOLBAR_BASE_MOCK_SCRIPT + r"""
window.__subjectiveTabCalls = [];
window.__subjectiveTabFailures = new Set();
const __baseInvokeForSubjectiveTab = window.__TAURI_INTERNALS__.invoke;

function subjectiveTabRow(overrides, finalOverrides = {}) {
  return Object.assign(toolbarRow(overrides), finalOverrides);
}

function subjectiveTabWorkbench() {
  if (new URLSearchParams(window.location.search).has("subjectiveEmpty")) {
    return { rows: [], attempts: [] };
  }
  const rubricPoints = JSON.stringify({
    schema_version: 1,
    rubric_points: [{
      source_public_id: "rubric-institution",
      stable_id: "institution",
      order_index: 0,
      canonical_text: "没有改变封建制度",
      max_score: 4,
    }],
  });
  const pointAnalysis = JSON.stringify({
    schema_version: 1,
    point_results: [{
      stable_id: "institution",
      status: "partial",
      suggested_score: 2,
      evidence_snippets: ["只学习技术"],
      reason: "表述不完整",
    }],
  });
  const teacherComponents = JSON.stringify({
    schema_version: 1,
    component_results: [{
      source_type: "rubric_point",
      source_public_id: "rubric-institution",
      stable_id: "institution",
      order_index: 0,
      teacher_score: 4,
      max_score: 4,
      result_status: "correct",
      evidence_text: "只学习技术，没有改变制度",
      teacher_note: "属于制度局限的合理表述",
    }],
  });

  return {
    rows: [
      subjectiveTabRow({
        student_id: 2,
        student_no: "2",
        student_name: "二号校正",
        answer_region_revision_id: 1002,
        transcription_revision_id: 2002,
        raw_ocr_text: "机器原文",
        normalized_text: "机器原文",
        suggestion_id: 3002,
        short_answer_analysis_id: null,
        machine_grade_ai_run_id: null,
        suggestion_outcome: "unscored",
        suggested_score: null,
        suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
      }, { rubric_points_json: rubricPoints }),
      subjectiveTabRow({
        student_id: 3,
        student_no: "03",
        student_name: "三号重试",
        answer_region_revision_id: 1003,
        transcription_revision_id: 2003,
        result_state: "recognize_failed",
        raw_ocr_text: null,
        normalized_text: null,
        confidence: null,
        suggestion_id: 3003,
        short_answer_analysis_id: null,
        machine_grade_ai_run_id: null,
        suggestion_outcome: "unscored",
        suggested_score: null,
        suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
      }, { rubric_points_json: rubricPoints }),
      subjectiveTabRow({
        student_id: 10,
        student_no: "10",
        student_name: "十号建议",
        answer_region_revision_id: 1010,
        transcription_revision_id: 2010,
        suggestion_id: 3010,
        short_answer_analysis_id: 4010,
        machine_grade_ai_run_id: 5010,
        suggestion_outcome: "partial",
        suggested_score: 2,
        suggestion_result_json: pointAnalysis,
      }, { rubric_points_json: rubricPoints }),
      subjectiveTabRow({
        student_id: 11,
        student_no: "11",
        student_name: "十一号总分",
        answer_region_revision_id: 1011,
        transcription_revision_id: 2011,
        raw_ocr_text: "只学习西方技术",
        normalized_text: "只学习西方技术",
        suggestion_id: 3011,
        short_answer_analysis_id: null,
        machine_grade_ai_run_id: null,
        suggestion_outcome: "unscored",
        suggested_score: null,
        suggestion_result_json: JSON.stringify({ schema_version: 1, point_results: [] }),
      }, { rubric_points_json: rubricPoints }),
      subjectiveTabRow({
        student_id: 13,
        student_no: "13",
        student_name: "十三号评分点晋级",
        answer_region_revision_id: 1013,
        transcription_revision_id: 2013,
        suggestion_id: 3013,
        short_answer_analysis_id: 4013,
        machine_grade_ai_run_id: 5013,
        suggestion_outcome: "partial",
        suggested_score: 2,
        suggestion_result_json: pointAnalysis,
        current_suggestion_confirmed: true,
      }, {
        rubric_points_json: rubricPoints,
        grade_decision_id: 7013,
        grade_decision_revision: 1,
        teacher_score: 4,
        confirmation_level: "teacher_corrected",
        review_mode: "teacher_corrected",
        teacher_components_json: teacherComponents,
      }),
      subjectiveTabRow({
        student_id: 14,
        student_no: "14",
        student_name: "十四号填空晋级",
        assessment_item_id: 102,
        order_index: 1,
        question_no: "2",
        question_type: "fill_blank",
        question_stem: "《南京条约》签订于____年",
        max_score: 2,
        answer_region_revision_id: 1014,
        transcription_revision_id: 2014,
        raw_ocr_text: "一八四二年",
        normalized_text: "1842年",
        teacher_corrected_text: "一八四二年",
        suggestion_id: 3014,
        suggestion_outcome: "incorrect",
        suggested_score: 0,
        current_suggestion_confirmed: true,
      }, {
        grade_decision_id: 7014,
        grade_decision_revision: 1,
        teacher_score: 2,
        confirmation_level: "teacher_corrected",
        review_mode: "teacher_corrected",
      }),
      subjectiveTabRow({
        assessment_id: 2,
        assessment_version_id: 20,
        assessment_title: "第二单元测验",
        student_id: 21,
        student_no: "21",
        student_name: "二十一号第二版本",
        assessment_item_id: 201,
        order_index: 0,
        question_no: "1",
        question_type: "fill_blank",
        question_stem: "辛亥革命爆发于____年",
        max_score: 2,
        answer_region_revision_id: 1021,
        transcription_revision_id: 2021,
        suggestion_id: 3021,
        suggestion_outcome: "correct",
        suggested_score: 2,
      }),
    ],
    attempts: [{
      assessment_id: 1,
      assessment_version_id: 10,
      assessment_title: "第一单元练习",
      attempt_id: 901,
      attempt_state: "ready_to_publish",
      active_publication_id: null,
      student_id: 90,
      student_no: "90",
      student_name: "九十号待发布",
      item_count: 2,
      observed_count: 2,
      confirmed_count: 2,
      teacher_total_score: 6,
      max_total_score: 6,
      published_total_score: null,
      can_publish: true,
    }],
  };
}

function recordSubjectiveTabCall(cmd, args) {
  window.__subjectiveTabCalls.push({ cmd, args: structuredClone(args) });
  if (window.__subjectiveTabFailures.has(cmd)) {
    throw new Error(`SIMULATED_${cmd}`);
  }
}

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "exam_answer_sheet_subjective_workbench") {
    return subjectiveTabWorkbench();
  }
  if (cmd === "exam_answer_sheet_correct_subjective_transcription") {
    recordSubjectiveTabCall(cmd, args);
    return {
      id: 9000 + args.answerRegionRevisionId,
      question_type: "short_answer",
      result_state: "recognized",
    };
  }
  if (cmd === "exam_answer_sheet_recognize_subjective_region") {
    recordSubjectiveTabCall(cmd, args);
    return {
      id: 9000 + args.answerRegionRevisionId,
      question_type: "short_answer",
      result_state: "recognized",
    };
  }
  if (cmd === "exam_answer_sheet_grade_short_answer") {
    recordSubjectiveTabCall(cmd, args);
    return { id: 5000 + args.transcriptionRevisionId };
  }
  if (cmd === "exam_answer_sheet_subjective_accept") {
    recordSubjectiveTabCall(cmd, args);
    return { id: 8000 + args.suggestionId, teacher_score: 2 };
  }
  if (cmd === "exam_answer_sheet_subjective_correct") {
    recordSubjectiveTabCall(cmd, args);
    return { id: 8000 + args.suggestionId, teacher_score: args.teacherScore };
  }
  if (cmd === "exam_answer_sheet_subjective_correct_components") {
    recordSubjectiveTabCall(cmd, args);
    return {
      id: 8000 + args.suggestionId,
      teacher_score: args.components.reduce(
        (total, component) => total + component.teacher_score,
        0,
      ),
    };
  }
  if (cmd === "exam_answer_sheet_promote_accepted_answer") {
    recordSubjectiveTabCall(cmd, args);
    return {
      outcome: "created_new_version",
      promotion_id: 9101,
      accepted_text: "一八四二年",
      adopted_assessment_version_id: 11,
      adopted_assessment_revision: 2,
      adopted_answer_key_version_id: 12,
      current_grade_unchanged: true,
      current_publication_unchanged: true,
    };
  }
  if (cmd === "exam_answer_sheet_promote_rubric_evidence") {
    recordSubjectiveTabCall(cmd, args);
    return {
      outcome: "created_new_version",
      promotion_id: 9102,
      rubric_point_stable_id: "institution",
      evidence_text: "只学习技术，没有改变制度",
      adopted_assessment_version_id: 11,
      adopted_assessment_revision: 2,
      adopted_rubric_version_id: 13,
      adopted_link_set_id: 14,
      carried_knowledge_link_count: 2,
      carried_ability_link_count: 1,
      current_grade_unchanged: true,
      current_publication_unchanged: true,
    };
  }
  if (cmd === "exam_answer_sheet_subjective_publish_attempt") {
    recordSubjectiveTabCall(cmd, args);
    return {
      id: 9901,
      attempt_id: args.attemptId,
      revision: 2,
      state: "published",
      total_score: 6,
      published_at: "2026-08-02T14:00:00Z",
    };
  }
  return __baseInvokeForSubjectiveTab(cmd, args);
};
"""


TARGET_COMMANDS = {
    "exam_answer_sheet_correct_subjective_transcription",
    "exam_answer_sheet_recognize_subjective_region",
    "exam_answer_sheet_grade_short_answer",
    "exam_answer_sheet_subjective_accept",
    "exam_answer_sheet_subjective_correct",
    "exam_answer_sheet_subjective_correct_components",
    "exam_answer_sheet_promote_accepted_answer",
    "exam_answer_sheet_promote_rubric_evidence",
    "exam_answer_sheet_subjective_publish_attempt",
}


def open_subjective_review(page: Page, url: str) -> None:
    page.add_init_script(SUBJECTIVE_TAB_MOCK_SCRIPT)
    page.goto(url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    page.get_by_role("button", name="答题卡主观题").click()


def student_card(page: Page, student_name: str):
    return page.locator("article.objective-review-row").filter(has_text=student_name)


def item_select(page: Page):
    toolbar = page.locator("section.objective-toolbar")
    return toolbar.locator("label.field").filter(has_text="按题终审").locator("select")


def version_select(page: Page):
    toolbar = page.locator("section.objective-toolbar")
    return toolbar.locator("label.field").filter(has_text="作业版本").locator("select")


def whole_score_editor(card):
    editor = card.locator("details").filter(has_text="仅记整题总分").first
    editor.locator("summary").click()
    return editor


def write_calls(page: Page) -> list[dict]:
    return page.evaluate("window.__subjectiveTabCalls")


def calls_for(page: Page, command: str) -> list[dict]:
    return [call for call in write_calls(page) if call["cmd"] == command]


def set_failure(page: Page, command: Optional[str]) -> None:
    page.evaluate(
        """command => {
          window.__subjectiveTabFailures.clear();
          if (command) window.__subjectiveTabFailures.add(command);
        }""",
        command,
    )


def assert_idempotency_key(value: str, prefix: str) -> None:
    assert re.fullmatch(
        re.escape(prefix) + r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}",
        value,
    )


def test_full_tab_success_and_guards(base_url: str, browser) -> None:
    page = browser.new_page(viewport={"width": 1500, "height": 1450})
    open_subjective_review(page, base_url)
    expect(page.locator("section.objective-toolbar")).to_be_visible()
    expect(page.locator("details.subjective-link-panel")).to_have_count(1)
    expect(page.locator(".objective-review-list")).to_be_visible()
    expect(page.locator(".sech").filter(has_text="整份答题卡发布")).to_be_visible()

    assert page.locator(".objective-review-list .objective-student b").all_text_contents() == [
        "二号校正",
        "三号重试",
        "十号建议",
        "十一号总分",
        "十三号评分点晋级",
    ]
    assert page.locator("section.objective-toolbar .objective-stats span").all_text_contents() == [
        "5 份作答",
        "1 已确认",
        "1 有明确建议",
        "3 需人工记分",
    ]

    version_select(page).select_option("20")
    expect(item_select(page)).to_have_value("201")
    expect(student_card(page, "二十一号第二版本")).to_be_visible()
    version_select(page).select_option("10")
    expect(item_select(page)).to_have_value("101")

    correction = student_card(page, "二号校正")
    correction.locator("summary").filter(has_text="校正机器原文").click()
    correction_input = correction.get_by_label("二号校正 第1题实际书写")
    correction_input.fill("   ")
    correction.get_by_role("button", name="按原图保存实际书写").click()
    expect(page.locator(".error")).to_have_text("请先按原图填写学生实际写下的内容")
    assert calls_for(page, "exam_answer_sheet_correct_subjective_transcription") == []

    correction.get_by_role("button", name="生成逐点评分建议").click()
    expect(page.locator(".ok-banner")).to_have_text("二号校正第1题已生成逐评分点建议，等待老师终审")
    direct_grade = calls_for(page, "exam_answer_sheet_grade_short_answer")[-1]
    assert direct_grade["args"]["transcriptionRevisionId"] == 2002
    assert_idempotency_key(
        direct_grade["args"]["idempotencyKey"],
        "answer-sheet:transcription:2002:answer-grade:",
    )

    correction_input.fill("  老师按原图校正  ")
    correction.get_by_role("button", name="按原图保存实际书写").click()
    expect(page.locator(".ok-banner")).to_have_text(
        "二号校正第1题已保存老师校正文本；机器原文仍保留；已按新文本生成逐点评分建议"
    )
    correction_call = calls_for(page, "exam_answer_sheet_correct_subjective_transcription")[-1]
    assert correction_call["args"] == {
        "answerRegionRevisionId": 1002,
        "correctedText": "老师按原图校正",
    }
    chained_grade = calls_for(page, "exam_answer_sheet_grade_short_answer")[-1]
    assert chained_grade["args"]["transcriptionRevisionId"] == 10002
    assert_idempotency_key(
        chained_grade["args"]["idempotencyKey"],
        "answer-sheet:transcription:10002:answer-grade:",
    )

    retry = student_card(page, "三号重试")
    retry.get_by_role("button", name="重新识别本题").click()
    expect(page.locator(".ok-banner")).to_have_text(
        "03号第1题已重新识别；旧转写仍保留；已生成逐点评分建议"
    )
    retry_call = calls_for(page, "exam_answer_sheet_recognize_subjective_region")[-1]
    assert retry_call["args"]["answerRegionRevisionId"] == 1003
    assert_idempotency_key(
        retry_call["args"]["idempotencyKey"],
        "answer-sheet:subjective:1003:retry:",
    )
    retry_grade = calls_for(page, "exam_answer_sheet_grade_short_answer")[-1]
    assert retry_grade["args"]["transcriptionRevisionId"] == 10003

    suggestion = student_card(page, "十号建议")
    suggestion.get_by_role("button", name="接受本条建议").click()
    expect(page.locator(".ok-banner")).to_have_text(
        "已终审 十号建议 的第1题；整份答题卡仍需显式发布"
    )
    assert calls_for(page, "exam_answer_sheet_subjective_accept")[-1]["args"] == {
        "suggestionId": 3010
    }

    total = student_card(page, "十一号总分")
    total_editor = whole_score_editor(total)
    score_input = total_editor.locator("label.field").filter(has_text="得分").locator("input")
    note_input = total_editor.locator("label.field").filter(has_text="判定依据").locator("input")
    score_input.fill("5")
    total_editor.get_by_role("button", name="保存人工 revision").click()
    expect(page.locator(".error")).to_have_text("人工得分必须位于 0~4 分")
    assert calls_for(page, "exam_answer_sheet_subjective_correct") == []
    score_input.fill("2.5")
    total_editor.get_by_role("button", name="保存人工 revision").click()
    expect(page.locator(".error")).to_have_text("人工记分必须填写查看原图或评分点后的判定依据")
    assert calls_for(page, "exam_answer_sheet_subjective_correct") == []
    note_input.fill("  覆盖制度局限但解释不完整  ")
    total_editor.get_by_role("button", name="保存人工 revision").click()
    expect(page.locator(".ok-banner")).to_have_text("已人工确认 十一号总分 的第1题为 2.5 分")
    assert calls_for(page, "exam_answer_sheet_subjective_correct")[-1]["args"] == {
        "suggestionId": 3011,
        "teacherScore": 2.5,
        "teacherNote": "覆盖制度局限但解释不完整",
    }

    rubric = student_card(page, "十三号评分点晋级")
    rubric_button = rubric.get_by_role("button", name="加入未来评分点示例")
    cancelled_rubric_dialogs: list[str] = []
    page.once("dialog", lambda dialog: (cancelled_rubric_dialogs.append(dialog.message), dialog.dismiss()))
    rubric_button.click()
    assert len(cancelled_rubric_dialogs) == 1
    assert "未来评分点示例" in cancelled_rubric_dialogs[0]
    assert calls_for(page, "exam_answer_sheet_promote_rubric_evidence") == []
    page.once("dialog", lambda dialog: dialog.accept())
    rubric_button.click()
    expect(page.locator(".ok-banner")).to_have_text(
        "已创建新评分规则：只学习技术，没有改变制度；沿用 2 条知识链接和 1 条能力链接，当前成绩保持不变"
    )
    assert calls_for(page, "exam_answer_sheet_promote_rubric_evidence")[-1]["args"] == {
        "gradeDecisionId": 7013,
        "sourcePublicId": "rubric-institution",
    }

    item_select(page).select_option("102")
    accepted = student_card(page, "十四号填空晋级")
    accepted_button = accepted.get_by_role("button", name="加入未来可接受答案")
    page.once("dialog", lambda dialog: dialog.dismiss())
    accepted_button.click()
    assert calls_for(page, "exam_answer_sheet_promote_accepted_answer") == []
    page.once("dialog", lambda dialog: dialog.accept())
    accepted_button.click()
    expect(page.locator(".ok-banner")).to_have_text(
        "已创建新答案版本：一八四二年；当前作业与历史成绩保持不变"
    )
    assert calls_for(page, "exam_answer_sheet_promote_accepted_answer")[-1]["args"] == {
        "gradeDecisionId": 7014
    }

    ready = page.locator("article.objective-attempt").filter(has_text="九十号待发布")
    publish_button = ready.get_by_role("button", name="确认发布整份答题卡")
    page.once("dialog", lambda dialog: dialog.dismiss())
    publish_button.click()
    assert calls_for(page, "exam_answer_sheet_subjective_publish_attempt") == []
    page.once("dialog", lambda dialog: dialog.accept())
    publish_button.click()
    expect(page.locator(".ok-banner")).to_have_text(
        "九十号待发布 的答题卡成绩已发布：6 分（revision 2）"
    )
    assert calls_for(page, "exam_answer_sheet_subjective_publish_attempt")[-1]["args"] == {
        "attemptId": 901
    }

    commands = {call["cmd"] for call in write_calls(page)}
    assert commands <= TARGET_COMMANDS
    page.screenshot(path="/tmp/jiaofu-r2-e11-subjective-review-tab-success.png", full_page=True)
    page.close()


def test_full_tab_failures_preserve_context(base_url: str, browser) -> None:
    page = browser.new_page(viewport={"width": 1500, "height": 1450})
    open_subjective_review(page, base_url)

    correction = student_card(page, "二号校正")
    correction.locator("summary").filter(has_text="校正机器原文").click()
    correction_input = correction.get_by_label("二号校正 第1题实际书写")
    correction_input.fill("失败后仍保留的校正")
    set_failure(page, "exam_answer_sheet_correct_subjective_transcription")
    correction.get_by_role("button", name="按原图保存实际书写").click()
    expect(page.locator(".error")).to_contain_text(
        "SIMULATED_exam_answer_sheet_correct_subjective_transcription"
    )
    expect(correction_input).to_have_value("失败后仍保留的校正")

    set_failure(page, "exam_answer_sheet_grade_short_answer")
    correction.get_by_role("button", name="按原图保存实际书写").click()
    expect(page.locator(".ok-banner")).to_contain_text("评分建议暂未生成")
    expect(page.locator(".ok-banner")).to_contain_text("已保存老师校正文本")
    expect(correction_input).to_have_value("失败后仍保留的校正")

    retry = student_card(page, "三号重试")
    set_failure(page, "exam_answer_sheet_recognize_subjective_region")
    retry.get_by_role("button", name="重新识别本题").click()
    expect(page.locator(".error")).to_contain_text(
        "SIMULATED_exam_answer_sheet_recognize_subjective_region"
    )
    expect(retry.get_by_role("button", name="重新识别本题")).to_be_enabled()

    set_failure(page, "exam_answer_sheet_grade_short_answer")
    retry.get_by_role("button", name="重新识别本题").click()
    expect(page.locator(".ok-banner")).to_contain_text("已重新识别；旧转写仍保留")
    expect(page.locator(".ok-banner")).to_contain_text("评分建议暂未生成")

    total = student_card(page, "十一号总分")
    set_failure(page, "exam_answer_sheet_grade_short_answer")
    total.get_by_role("button", name="生成逐点评分建议").click()
    expect(page.locator(".error")).to_contain_text("SIMULATED_exam_answer_sheet_grade_short_answer")
    expect(total.get_by_role("button", name="生成逐点评分建议")).to_be_enabled()

    suggestion = student_card(page, "十号建议")
    set_failure(page, "exam_answer_sheet_subjective_accept")
    suggestion.get_by_role("button", name="接受本条建议").click()
    expect(page.locator(".error")).to_contain_text("SIMULATED_exam_answer_sheet_subjective_accept")
    expect(suggestion.get_by_role("button", name="接受本条建议")).to_be_enabled()

    component_editor = suggestion.locator("details").filter(has_text="按评分点逐项确认").first
    overall_note = component_editor.locator("label.field").filter(
        has_text="本题整体判定依据"
    ).locator("input")
    overall_note.fill("逐项失败后仍保留")
    set_failure(page, "exam_answer_sheet_subjective_correct_components")
    component_editor.get_by_role("button", name="保存逐项结论并自动汇总").click()
    expect(page.locator(".error")).to_contain_text(
        "SIMULATED_exam_answer_sheet_subjective_correct_components"
    )
    expect(overall_note).to_have_value("逐项失败后仍保留")

    total_editor = whole_score_editor(total)
    score_input = total_editor.locator("label.field").filter(has_text="得分").locator("input")
    note_input = total_editor.locator("label.field").filter(has_text="判定依据").locator("input")
    score_input.fill("1.5")
    note_input.fill("总分失败后仍保留")
    set_failure(page, "exam_answer_sheet_subjective_correct")
    total_editor.get_by_role("button", name="保存人工 revision").click()
    expect(page.locator(".error")).to_contain_text("SIMULATED_exam_answer_sheet_subjective_correct")
    expect(score_input).to_have_value("1.5")
    expect(note_input).to_have_value("总分失败后仍保留")

    rubric = student_card(page, "十三号评分点晋级")
    set_failure(page, "exam_answer_sheet_promote_rubric_evidence")
    page.once("dialog", lambda dialog: dialog.accept())
    rubric.get_by_role("button", name="加入未来评分点示例").click()
    expect(page.locator(".error")).to_contain_text(
        "SIMULATED_exam_answer_sheet_promote_rubric_evidence"
    )
    expect(rubric.get_by_role("button", name="加入未来评分点示例")).to_be_enabled()

    item_select(page).select_option("102")
    accepted = student_card(page, "十四号填空晋级")
    set_failure(page, "exam_answer_sheet_promote_accepted_answer")
    page.once("dialog", lambda dialog: dialog.accept())
    accepted.get_by_role("button", name="加入未来可接受答案").click()
    expect(page.locator(".error")).to_contain_text(
        "SIMULATED_exam_answer_sheet_promote_accepted_answer"
    )
    expect(accepted.get_by_role("button", name="加入未来可接受答案")).to_be_enabled()

    ready = page.locator("article.objective-attempt").filter(has_text="九十号待发布")
    set_failure(page, "exam_answer_sheet_subjective_publish_attempt")
    page.once("dialog", lambda dialog: dialog.accept())
    ready.get_by_role("button", name="确认发布整份答题卡").click()
    expect(page.locator(".error")).to_contain_text(
        "SIMULATED_exam_answer_sheet_subjective_publish_attempt"
    )
    expect(ready.get_by_role("button", name="确认发布整份答题卡")).to_be_enabled()

    commands = {call["cmd"] for call in write_calls(page)}
    assert TARGET_COMMANDS <= commands
    assert commands <= TARGET_COMMANDS
    page.screenshot(path="/tmp/jiaofu-r2-e11-subjective-review-tab-failures.png", full_page=True)
    page.close()


def test_empty_state(base_url: str, browser) -> None:
    page = browser.new_page(viewport={"width": 1400, "height": 900})
    open_subjective_review(page, f"{base_url}?subjectiveEmpty=1")
    empty = page.locator(".empty-state.objective-empty")
    expect(empty.get_by_text("还没有答题卡主观题转写。", exact=True)).to_be_visible()
    expect(empty.get_by_text(
        "上传并处理答题卡后，填空题会做确定性答案比对；篇幅受控简答题会自动按评分点整理原文证据。",
        exact=True,
    )).to_be_visible()
    expect(page.locator("section.objective-toolbar")).to_have_count(0)
    expect(page.locator(".objective-review-list")).to_have_count(0)
    expect(empty.get_by_role("button", name=re.compile("终审|发布|重新识别|评分建议"))).to_have_count(0)
    assert write_calls(page) == []
    page.screenshot(path="/tmp/jiaofu-r2-e11-subjective-review-tab-empty.png", full_page=True)
    page.close()


if __name__ == "__main__":
    with sync_playwright() as playwright:
        test_browser = playwright.chromium.launch(headless=True)
        test_full_tab_success_and_guards("http://127.0.0.1:4173/", test_browser)
        test_full_tab_failures_preserve_context("http://127.0.0.1:4173/", test_browser)
        test_empty_state("http://127.0.0.1:4173/", test_browser)
        test_browser.close()
    print("exam subjective review tab UI characterization: PASS")
