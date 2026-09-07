"""固定默写终审 Tab 的筛选、证据、校正、终审、重试和发布行为。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_exam_knowledge_tab_ui import MOCK_SCRIPT as BASE_MOCK_SCRIPT


DICTATION_MOCK_SCRIPT = BASE_MOCK_SCRIPT + r"""
window.__dictationCalls = [];
window.__dictationRows = [];
window.__dictationAttempts = [];

function dictationRow(overrides = {}) {
  return {
    assessment_id: 1,
    assessment_version_id: 100,
    assessment_title: "八上历史第一次默写",
    attempt_id: 5001,
    attempt_state: "grading",
    active_publication_id: null,
    student_id: 1,
    student_no: "01",
    student_name: "默认同学",
    assessment_item_id: 1001,
    order_index: 1,
    question_no: "1",
    question_type: "fill_blank",
    question_stem: "《南京条约》签订年份",
    max_score: 2,
    answer_region_revision_id: 8001,
    crop_path: "/tmp/dictation-default.png",
    transcription_revision_id: 9001,
    transcription_revision: 1,
    source_ai_run_id: 7001,
    result_state: "recognized",
    raw_ocr_text: "1842年",
    normalized_text: "1842年",
    teacher_corrected_text: null,
    confidence: 0.98,
    point_result: "exact",
    canonical_text: "1842年",
    accepted_variants: ["一八四二年"],
    suggested_score: 2,
    requires_teacher_review: false,
    grade_decision_id: null,
    grade_decision_revision: null,
    teacher_score: null,
    confirmation_level: null,
    review_mode: null,
    current_transcription_confirmed: false,
    decided_at: null,
    ...overrides,
  };
}

function dictationAttempt(overrides = {}) {
  return {
    assessment_id: 1,
    assessment_version_id: 100,
    assessment_title: "八上历史第一次默写",
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

window.__dictationRows = [
  dictationRow({
    student_id: 2,
    student_no: "2",
    student_name: "二号涂改",
    attempt_id: 5002,
    answer_region_revision_id: 8002,
    crop_path: null,
    transcription_revision_id: 9002,
    result_state: "ambiguous_final",
    raw_ocr_text: "1840年（划去）",
    normalized_text: null,
    confidence: 0.72,
    point_result: "needs_review",
    suggested_score: null,
    requires_teacher_review: true,
  }),
  dictationRow({
    student_id: 3,
    student_no: "3",
    student_name: "三号变体已确认",
    attempt_id: 5003,
    answer_region_revision_id: 8003,
    transcription_revision_id: 9003,
    raw_ocr_text: "一八四二年",
    normalized_text: "1842年",
    confidence: 0.99,
    point_result: "accepted_variant",
    grade_decision_id: 9303,
    grade_decision_revision: 1,
    teacher_score: 2,
    confirmation_level: "teacher_accepted",
    review_mode: "strict_batch",
    current_transcription_confirmed: true,
    decided_at: "2026-08-02T09:00:00Z",
  }),
  dictationRow({
    student_id: 10,
    student_no: "10",
    student_name: "十号精确",
    attempt_id: 5010,
    answer_region_revision_id: 8010,
    transcription_revision_id: 9010,
  }),
  dictationRow({
    student_id: 11,
    student_no: "11",
    student_name: "十一号识别失败",
    attempt_id: 5011,
    answer_region_revision_id: 8011,
    crop_path: null,
    transcription_revision_id: 9011,
    result_state: "recognize_failed",
    raw_ocr_text: null,
    normalized_text: null,
    confidence: null,
    point_result: "recognize_failed",
    suggested_score: null,
    requires_teacher_review: true,
  }),
  dictationRow({
    student_id: 12,
    student_no: "12",
    student_name: "十二号未写",
    attempt_id: 5012,
    answer_region_revision_id: 8012,
    transcription_revision_id: 9012,
    result_state: "not_written",
    raw_ocr_text: null,
    normalized_text: null,
    confidence: 0.99,
    point_result: "not_written",
    suggested_score: 0,
    requires_teacher_review: true,
  }),
  dictationRow({
    student_id: 13,
    student_no: "13",
    student_name: "十三号难辨",
    attempt_id: 5013,
    answer_region_revision_id: 8013,
    transcription_revision_id: 9013,
    result_state: "unreadable",
    raw_ocr_text: null,
    normalized_text: null,
    confidence: 0.41,
    point_result: "unreadable",
    suggested_score: null,
    requires_teacher_review: true,
  }),
  dictationRow({
    student_id: 14,
    student_no: "14",
    student_name: "十四号有分歧",
    attempt_id: 5014,
    answer_region_revision_id: 8014,
    transcription_revision_id: 9014,
    raw_ocr_text: "1840年",
    normalized_text: "1840年",
    confidence: 0.94,
    point_result: "needs_review",
    suggested_score: 0,
    requires_teacher_review: true,
  }),
  dictationRow({
    student_id: 15,
    student_no: "15",
    student_name: "十五号第二题",
    attempt_id: 5015,
    assessment_item_id: 1002,
    order_index: 2,
    question_no: "2",
    question_type: "short_answer",
    question_stem: "写出洋务运动前期口号",
    max_score: 1,
    answer_region_revision_id: 8015,
    transcription_revision_id: 9015,
    raw_ocr_text: "自强",
    normalized_text: "自强",
    confidence: 0.98,
    point_result: "accepted_variant",
    canonical_text: "自强",
    accepted_variants: ["自强口号"],
    suggested_score: 1,
  }),
  dictationRow({
    assessment_id: 2,
    assessment_version_id: 200,
    assessment_title: "八上历史第二次默写",
    student_id: 20,
    student_no: "20",
    student_name: "二十号第二作业",
    attempt_id: 5020,
    assessment_item_id: 2001,
    order_index: 1,
    question_no: "1",
    question_stem: "戊戌变法开始年份",
    answer_region_revision_id: 8020,
    transcription_revision_id: 9020,
    raw_ocr_text: "1898年",
    normalized_text: "1898年",
    canonical_text: "1898年",
    accepted_variants: ["一八九八年"],
  }),
];

window.__dictationAttempts = [
  dictationAttempt({
    attempt_id: 5002,
    student_id: 2,
    student_no: "2",
    student_name: "二号涂改",
    confirmed_count: 2,
    teacher_total_score: 3,
    can_publish: true,
    attempt_state: "ready_to_publish",
  }),
  dictationAttempt({
    attempt_id: 5003,
    student_id: 3,
    student_no: "3",
    student_name: "三号变体已确认",
    confirmed_count: 1,
    teacher_total_score: 2,
  }),
  dictationAttempt({
    assessment_id: 2,
    assessment_version_id: 200,
    assessment_title: "八上历史第二次默写",
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

function currentDictationWorkbench() {
  if (window.location.search.includes("dictationEmpty=1")) {
    return { rows: [], attempts: [] };
  }
  return {
    rows: window.__dictationRows.map((row) => ({ ...row, accepted_variants: [...row.accepted_variants] })),
    attempts: window.__dictationAttempts.map((attempt) => ({ ...attempt })),
  };
}

function dictationFailureRequested() {
  return window.location.search.includes("dictationFail=1");
}

function confirmDictationRow(row, score, confirmationLevel, reviewMode) {
  row.current_transcription_confirmed = true;
  row.teacher_score = score;
  row.confirmation_level = confirmationLevel;
  row.review_mode = reviewMode;
  row.grade_decision_id = 9300 + row.transcription_revision_id;
  row.grade_decision_revision = 1;
  row.decided_at = "2026-08-02T10:00:00Z";
}

const __baseInvokeForDictationReview = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "exam_dictation_workbench") return currentDictationWorkbench();

  if (cmd === "exam_dictation_strict_batch_accept") {
    window.__dictationCalls.push({ cmd, args });
    if (dictationFailureRequested()) throw new Error("模拟默写严格批量失败");
    const eligible = window.__dictationRows.filter((row) => (
      args.transcriptionRevisionIds.includes(row.transcription_revision_id)
      && !row.current_transcription_confirmed
      && !row.requires_teacher_review
      && row.suggested_score != null
      && row.result_state === "recognized"
      && row.confidence != null
      && row.confidence >= 0.95
    ));
    eligible.forEach((row) => confirmDictationRow(row, row.suggested_score, "teacher_accepted", "strict_batch"));
    return {
      id: 9501,
      requested_count: args.transcriptionRevisionIds.length,
      confirmed_count: eligible.length,
      excluded_count: args.transcriptionRevisionIds.length - eligible.length,
      items: args.transcriptionRevisionIds.map((transcriptionRevisionId) => ({
        transcription_revision_id: transcriptionRevisionId,
        outcome: eligible.some((row) => row.transcription_revision_id === transcriptionRevisionId) ? "confirmed" : "excluded",
      })),
    };
  }

  if (cmd === "exam_dictation_correct_transcription") {
    window.__dictationCalls.push({ cmd, args });
    if (dictationFailureRequested()) throw new Error("模拟默写原文校正失败");
    const row = window.__dictationRows.find((item) => item.answer_region_revision_id === args.answerRegionRevisionId);
    row.teacher_corrected_text = args.correctedText;
    row.transcription_revision += 1;
    return {
      transcription: {
        id: row.transcription_revision_id,
        answer_region_revision_id: row.answer_region_revision_id,
        revision: row.transcription_revision,
        result_state: row.result_state,
        raw_ocr_text: row.raw_ocr_text,
        normalized_text: row.normalized_text,
        teacher_corrected_text: row.teacher_corrected_text,
        confidence: row.confidence,
      },
      observation: { id: 9601, result: "needs_review", suggested_score: row.suggested_score },
    };
  }

  if (cmd === "exam_dictation_recognize_region") {
    window.__dictationCalls.push({ cmd, args });
    if (dictationFailureRequested()) throw new Error("模拟默写重新识别失败");
    const row = window.__dictationRows.find((item) => item.answer_region_revision_id === args.answerRegionRevisionId);
    return {
      transcription: {
        id: row.transcription_revision_id,
        answer_region_revision_id: row.answer_region_revision_id,
        revision: row.transcription_revision,
        result_state: row.result_state,
        raw_ocr_text: row.raw_ocr_text,
        normalized_text: row.normalized_text,
        teacher_corrected_text: row.teacher_corrected_text,
        confidence: row.confidence,
      },
      observation: { id: 9602, result: "recognize_failed", suggested_score: null },
    };
  }

  if (cmd === "exam_dictation_accept") {
    window.__dictationCalls.push({ cmd, args });
    if (dictationFailureRequested()) throw new Error("模拟默写逐条接受失败");
    const row = window.__dictationRows.find((item) => item.transcription_revision_id === args.transcriptionRevisionId);
    confirmDictationRow(row, row.suggested_score, "teacher_accepted", "single");
    return { id: row.grade_decision_id, revision: 1, teacher_score: row.teacher_score };
  }

  if (cmd === "exam_dictation_correct_grade") {
    window.__dictationCalls.push({ cmd, args });
    if (dictationFailureRequested()) throw new Error("模拟默写人工记分失败");
    const row = window.__dictationRows.find((item) => item.transcription_revision_id === args.transcriptionRevisionId);
    confirmDictationRow(row, args.teacherScore, "teacher_corrected", "single");
    return { id: row.grade_decision_id, revision: 1, teacher_score: row.teacher_score };
  }

  if (cmd === "exam_dictation_publish_attempt") {
    window.__dictationCalls.push({ cmd, args });
    if (dictationFailureRequested()) throw new Error("模拟默写整份发布失败");
    const attempt = window.__dictationAttempts.find((item) => item.attempt_id === args.attemptId);
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

  return __baseInvokeForDictationReview(cmd, args);
};
"""


def reach_dictation_tab(page: Page, url: str) -> None:
    page.goto(url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()
    page.get_by_role("button", name="默写复核", exact=True).click()
    expect(page.get_by_text("本题证据", exact=False)).to_be_visible()
    expect(page.get_by_text("整份默写发布", exact=False)).to_be_visible()


def dictation_row(page: Page, student_name: str):
    return page.locator(".objective-review-row").filter(has_text=student_name)


def test_dictation_review_success(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 2200})
        page.add_init_script(DICTATION_MOCK_SCRIPT)
        reach_dictation_tab(page, base_url)

        assessment_select = page.get_by_label("作业版本")
        item_select = page.get_by_label("按题终审")
        expect(assessment_select.locator("option")).to_have_count(2)
        expect(item_select.locator("option")).to_have_count(2)
        expect(item_select.locator("option").first).to_contain_text("第1题")
        expect(item_select.locator("option").nth(1)).to_contain_text("第2题")

        stats = page.locator(".objective-stats")
        expect(stats).to_contain_text("7 份作答")
        expect(stats).to_contain_text("1 已确认")
        expect(stats).to_contain_text("1 可严格批量")
        expect(stats).to_contain_text("5 需单独处理")
        expect(page.locator(".objective-batch-note")).to_contain_text("置信度不低于 0.95")
        expect(page.locator(".objective-batch-note")).to_contain_text("未写、模糊、涂改、分歧和已终审项都会逐条排除")

        names = page.locator(".objective-student b").all_text_contents()
        assert names == ["二号涂改", "三号变体已确认", "十号精确", "十一号识别失败", "十二号未写", "十三号难辨", "十四号有分歧"]

        altered = dictation_row(page, "二号涂改")
        expect(altered).to_contain_text("裁剪图不可用")
        expect(altered).to_contain_text("涂改后答案不明确")
        expect(altered).to_contain_text("机器原文 1840年（划去）")
        confirmed = dictation_row(page, "三号变体已确认")
        expect(confirmed).to_contain_text("可接受写法")
        expect(confirmed).to_contain_text("老师终审 2 分 · 严格批量")
        exact = dictation_row(page, "十号精确")
        expect(exact).to_contain_text("与标准答案一致")
        expect(exact.locator("img.objective-crop")).to_have_attribute("src", "/tmp/dictation-default.png")
        failed = dictation_row(page, "十一号识别失败")
        expect(failed).to_contain_text("识别失败")
        expect(failed.get_by_role("button", name="重新识别本题")).to_be_visible()
        expect(dictation_row(page, "十二号未写")).to_contain_text("疑似未写")
        expect(dictation_row(page, "十三号难辨")).to_contain_text("字迹无法辨认")
        disputed = dictation_row(page, "十四号有分歧")
        expect(disputed).to_contain_text("与标准答案有分歧")
        expect(disputed).to_contain_text("标准内容 1842年")
        expect(disputed).to_contain_text("可接受写法 一八四二年")

        page.get_by_role("button", name="确认 1 条精确结果").click()
        expect(page.locator(".ok-banner")).to_have_text("严格批量终审完成：确认 1 条，排除 6 条分歧")
        expect(stats).to_contain_text("2 已确认")
        expect(stats).to_contain_text("0 可严格批量")
        batch_call = page.evaluate("window.__dictationCalls[0]")
        assert batch_call["cmd"] == "exam_dictation_strict_batch_accept"
        assert batch_call["args"]["transcriptionRevisionIds"] == [9002, 9003, 9010, 9011, 9012, 9013, 9014]
        assert batch_call["args"]["idempotencyKey"].startswith("dictation-review-")

        disputed.get_by_text("校正机器原文", exact=True).click()
        correction_input = disputed.get_by_label("十四号有分歧 第1题实际书写")
        correction_input.fill("")
        disputed.get_by_role("button", name="按原图保存实际书写").click()
        expect(page.locator(".error")).to_have_text("请先按原图填写学生实际写下的内容")
        assert page.evaluate("window.__dictationCalls.length") == 1

        correction_input.fill(" 1842年 ")
        disputed.get_by_role("button", name="按原图保存实际书写").click()
        expect(page.locator(".ok-banner")).to_have_text("十四号有分歧第1题已保存实际书写；请再确认得分，机器原文仍保留")
        assert page.evaluate("window.__dictationCalls[1]") == {
            "cmd": "exam_dictation_correct_transcription",
            "args": {"answerRegionRevisionId": 8014, "correctedText": "1842年"},
        }
        expect(disputed).to_contain_text("机器原文 1840年")
        expect(disputed).to_contain_text("老师校正 1842年")

        altered.get_by_text("人工记分 / 补录", exact=True).click()
        score_input = altered.get_by_label("得分（满分 2）")
        evidence_input = altered.get_by_label("学生实际书写（可选）")
        note_input = altered.get_by_label("判定依据（必填）")
        score_input.fill("3")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".error")).to_have_text("人工得分必须位于 0~2 分")
        assert page.evaluate("window.__dictationCalls.length") == 2

        score_input.fill("1")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".error")).to_have_text("人工记分必须填写查看原图后的判定依据")
        assert page.evaluate("window.__dictationCalls.length") == 2

        evidence_input.fill(" 一八四零年（划去） ")
        note_input.fill(" 原图涂改最终无法确认 ")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".ok-banner")).to_have_text("已人工确认 二号涂改 的第1题为 1 分")
        assert page.evaluate("window.__dictationCalls[2]") == {
            "cmd": "exam_dictation_correct_grade",
            "args": {
                "transcriptionRevisionId": 9002,
                "teacherScore": 1,
                "teacherNote": "原图涂改最终无法确认",
                "teacherEvidenceText": "一八四零年（划去）",
            },
        }

        failed.get_by_role("button", name="重新识别本题").click()
        expect(page.locator(".ok-banner")).to_have_text("11号第1题已重新识别；旧失败记录仍保留")
        retry_call = page.evaluate("window.__dictationCalls[3]")
        assert retry_call["cmd"] == "exam_dictation_recognize_region"
        assert retry_call["args"]["answerRegionRevisionId"] == 8011
        assert retry_call["args"]["idempotencyKey"].startswith("dictation:region:8011:retry:")

        item_select.select_option("1002")
        expect(page.locator(".objective-review-row")).to_have_count(1)
        second_item = dictation_row(page, "十五号第二题")
        expect(second_item).to_contain_text("可接受写法")
        second_item.get_by_role("button", name="接受本条建议").click()
        expect(page.locator(".ok-banner")).to_have_text("已终审 十五号第二题 的第2题；整份默写仍需显式发布")
        assert page.evaluate("window.__dictationCalls[4]") == {
            "cmd": "exam_dictation_accept",
            "args": {"transcriptionRevisionId": 9015},
        }

        assessment_select.select_option("200")
        expect(item_select.locator("option")).to_have_count(1)
        expect(item_select).to_have_value("2001")
        expect(page.locator(".objective-review-row")).to_have_count(1)
        expect(dictation_row(page, "二十号第二作业")).to_contain_text("1898年")
        assessment_select.select_option("100")

        publish_card = page.locator(".objective-attempt").filter(has_text="二号涂改")
        publish_button = publish_card.get_by_role("button", name="确认发布整份默写")
        page.once("dialog", lambda dialog: dialog.dismiss())
        publish_button.click()
        assert page.evaluate("window.__dictationCalls.length") == 5

        page.once("dialog", lambda dialog: dialog.accept())
        publish_button.click()
        expect(page.locator(".ok-banner")).to_have_text("二号涂改 的默写成绩已发布：3 分（revision 2）")
        assert page.evaluate("window.__dictationCalls[5]") == {
            "cmd": "exam_dictation_publish_attempt",
            "args": {"attemptId": 5002},
        }
        expect(publish_card).to_contain_text("已发布")
        expect(publish_card).to_contain_text("当前已发布总分：3")

        write_commands = page.evaluate("window.__dictationCalls.map((item) => item.cmd)")
        assert write_commands == [
            "exam_dictation_strict_batch_accept",
            "exam_dictation_correct_transcription",
            "exam_dictation_correct_grade",
            "exam_dictation_recognize_region",
            "exam_dictation_accept",
            "exam_dictation_publish_attempt",
        ]
        page.screenshot(path="/tmp/jiaofu-r2-dictation-review-success.png", full_page=True)
        browser.close()


def test_dictation_review_failures(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 2200})
        page.add_init_script(DICTATION_MOCK_SCRIPT)
        reach_dictation_tab(page, f"{base_url}?dictationFail=1")

        stats = page.locator(".objective-stats")
        page.get_by_role("button", name="确认 1 条精确结果").click()
        expect(page.locator(".error")).to_contain_text("模拟默写严格批量失败")
        expect(stats).to_contain_text("1 已确认")
        expect(stats).to_contain_text("1 可严格批量")

        disputed = dictation_row(page, "十四号有分歧")
        disputed.get_by_text("校正机器原文", exact=True).click()
        correction_input = disputed.get_by_label("十四号有分歧 第1题实际书写")
        correction_input.fill("保留这段老师校正")
        disputed.get_by_role("button", name="按原图保存实际书写").click()
        expect(page.locator(".error")).to_contain_text("模拟默写原文校正失败")
        expect(correction_input).to_have_value("保留这段老师校正")
        expect(disputed).to_contain_text("需单独处理")

        altered = dictation_row(page, "二号涂改")
        altered.get_by_text("人工记分 / 补录", exact=True).click()
        altered.get_by_label("得分（满分 2）").fill("1")
        altered.get_by_label("学生实际书写（可选）").fill("保留这段学生书写")
        altered.get_by_label("判定依据（必填）").fill("保留这段老师依据")
        altered.get_by_role("button", name="保存人工 revision").click()
        expect(page.locator(".error")).to_contain_text("模拟默写人工记分失败")
        expect(altered.get_by_label("得分（满分 2）")).to_have_value("1")
        expect(altered.get_by_label("学生实际书写（可选）")).to_have_value("保留这段学生书写")
        expect(altered.get_by_label("判定依据（必填）")).to_have_value("保留这段老师依据")

        failed = dictation_row(page, "十一号识别失败")
        failed.get_by_role("button", name="重新识别本题").click()
        expect(page.locator(".error")).to_contain_text("模拟默写重新识别失败")
        expect(failed.get_by_role("button", name="重新识别本题")).to_be_visible()

        page.get_by_label("按题终审").select_option("1002")
        second_item = dictation_row(page, "十五号第二题")
        second_item.get_by_role("button", name="接受本条建议").click()
        expect(page.locator(".error")).to_contain_text("模拟默写逐条接受失败")
        expect(second_item.get_by_role("button", name="接受本条建议")).to_be_enabled()

        publish_card = page.locator(".objective-attempt").filter(has_text="二号涂改")
        page.once("dialog", lambda dialog: dialog.accept())
        publish_card.get_by_role("button", name="确认发布整份默写").click()
        expect(page.locator(".error")).to_contain_text("模拟默写整份发布失败")
        expect(publish_card).to_contain_text("待发布")
        expect(publish_card.get_by_role("button", name="确认发布整份默写")).to_be_enabled()

        assert page.evaluate("window.__dictationCalls.map((item) => item.cmd)") == [
            "exam_dictation_strict_batch_accept",
            "exam_dictation_correct_transcription",
            "exam_dictation_correct_grade",
            "exam_dictation_recognize_region",
            "exam_dictation_accept",
            "exam_dictation_publish_attempt",
        ]
        expect(page.locator(".ok-banner")).to_have_count(0)
        page.screenshot(path="/tmp/jiaofu-r2-dictation-review-failures.png", full_page=True)
        browser.close()


def test_dictation_review_empty(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        page.add_init_script(DICTATION_MOCK_SCRIPT)
        page.goto(f"{base_url}?dictationEmpty=1")
        page.wait_for_load_state("networkidle")
        page.locator(".mod-row").filter(has_text="改作业").click()
        page.get_by_role("button", name="题目批改").click()
        page.get_by_role("button", name="默写复核", exact=True).click()

        empty = page.locator(".objective-empty")
        expect(empty).to_contain_text("还没有可终审的默写结果")
        expect(empty).to_contain_text("上传固定默写后，系统会按已确认模板裁出每个答案区")
        expect(empty).to_contain_text("精确结果可批量确认，分歧只需老师查看原图")
        expect(page.locator(".objective-review-row")).to_have_count(0)
        expect(page.get_by_role("button", name="确认发布整份默写")).to_have_count(0)
        assert page.evaluate("window.__dictationCalls") == []
        browser.close()


if __name__ == "__main__":
    test_dictation_review_success("http://127.0.0.1:4173")
    test_dictation_review_failures("http://127.0.0.1:4173")
    test_dictation_review_empty("http://127.0.0.1:4173")
    print("exam DictationReviewTab UI characterization: PASS")
