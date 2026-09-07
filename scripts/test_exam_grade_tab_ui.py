"""固定 GradeTab 的机器建议、老师终审、改判和失败保留行为。"""

from playwright.sync_api import expect, sync_playwright

from test_exam_knowledge_tab_ui import MOCK_SCRIPT as BASE_MOCK_SCRIPT


GRADE_MOCK_SCRIPT = BASE_MOCK_SCRIPT + r"""
window.__gradeCalls = [];
window.__gradeStudents = [
  { id: 1, student_no: "01", name: "张三", class_name: "八年级一班", enabled: true },
  { id: 2, student_no: "02", name: "李四", class_name: "八年级一班", enabled: true }
];
window.__gradeQuestions = [
  {
    id: 7, subject_id: null, question_no: "HIS-001", qtype: "single",
    stem: "洋务运动前期口号是？", image_path: null, correct_answer: "A",
    knowledge_point_id: 2, difficulty: null, analysis: null, max_score: 2, enabled: true
  },
  {
    id: 8, subject_id: null, question_no: "HIS-002", qtype: "fill",
    stem: "鸦片战争爆发于哪一年？", image_path: null, correct_answer: "1840",
    knowledge_point_id: 1, difficulty: null, analysis: null, max_score: 1, enabled: true
  },
  {
    id: 9, subject_id: null, question_no: "HIS-003", qtype: "subjective",
    stem: "分析洋务运动失败原因", image_path: null, correct_answer: "制度局限",
    knowledge_point_id: 2, difficulty: null, analysis: null, max_score: 4, enabled: true
  }
];

function gradeAnswer({
  id, studentId, questionId, picked, machineCorrect, humanCorrect = null,
  status = "pending_review", humanNote = null, createdAt = "2026-08-02T08:30:00Z"
}) {
  const student = window.__gradeStudents.find((item) => item.id === studentId);
  const question = window.__gradeQuestions.find((item) => item.id === questionId);
  return {
    id,
    student_id: studentId,
    student_name: student.name,
    question_id: questionId,
    question_no: question.question_no,
    question_stem: question.stem,
    question_type: question.qtype,
    picked,
    correct_answer: question.correct_answer,
    machine_correct: machineCorrect,
    human_correct: humanCorrect,
    is_correct: humanCorrect,
    score: humanCorrect == null ? null : (humanCorrect ? question.max_score : 0),
    max_score: question.max_score,
    knowledge_point_id: question.knowledge_point_id,
    knowledge_point_name: questionId === 7 ? "洋务运动" : "中国近代史",
    status,
    machine_note: machineCorrect ? "标准化答案一致" : "标准化答案不一致",
    human_note: humanNote,
    created_at: createdAt,
    decided_at: status === "confirmed" ? "2026-08-02T08:40:00Z" : null
  };
}

window.__gradeAnswers = [
  gradeAnswer({ id: 101, studentId: 1, questionId: 7, picked: "B", machineCorrect: false }),
  gradeAnswer({
    id: 102, studentId: 2, questionId: 8, picked: "1840", machineCorrect: true,
    humanCorrect: true, status: "confirmed", humanNote: "已核对", createdAt: "2026-08-02T08:20:00Z"
  })
];

const baseGradeInvoke = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "students_list") return window.__gradeStudents;
  if (cmd === "questions_list") return window.__gradeQuestions;
  if (cmd === "exam_answers_list") return window.__gradeAnswers;
  if (cmd === "exam_answer_suggest") {
    window.__gradeCalls.push({ cmd, args });
    if (window.location.search.includes("gradeSuggestFail=1")) {
      throw new Error("模拟机器建议失败");
    }
    const question = window.__gradeQuestions.find((item) => item.id === args.questionId);
    const normalized = String(args.picked).trim().toUpperCase();
    const answer = gradeAnswer({
      id: 100 + window.__gradeAnswers.length + 1,
      studentId: args.studentId,
      questionId: args.questionId,
      picked: args.picked,
      machineCorrect: normalized === question.correct_answer.toUpperCase(),
      createdAt: "2026-08-02T08:50:00Z"
    });
    window.__gradeAnswers.push(answer);
    return answer;
  }
  if (cmd === "exam_answer_human_decide") {
    window.__gradeCalls.push({ cmd, args });
    if (window.location.search.includes("gradeDecideFail=1")) {
      throw new Error("模拟老师终审失败");
    }
    const index = window.__gradeAnswers.findIndex((item) => item.id === args.answerId);
    const answer = window.__gradeAnswers[index];
    const updated = {
      ...answer,
      status: "confirmed",
      human_correct: args.isCorrect,
      is_correct: args.isCorrect,
      score: args.isCorrect ? answer.max_score : 0,
      human_note: args.note,
      decided_at: "2026-08-02T09:00:00Z"
    };
    window.__gradeAnswers[index] = updated;
    return updated;
  }
  return baseGradeInvoke(cmd, args);
};
"""


def reach_grade_tab(page, url: str) -> None:
    page.goto(url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    expect(page.get_by_role("heading", name="题目批改")).to_be_visible()
    page.get_by_role("button", name="老师补录", exact=True).click()
    expect(page.get_by_text("录入一题作答", exact=True)).to_be_visible()
    expect(page.locator(".sech").filter(has_text="终审队列")).to_be_visible()


def test_grade_tab_success(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1200})
        page.add_init_script(GRADE_MOCK_SCRIPT)
        reach_grade_tab(page, base_url)

        student_select = page.get_by_label("学生")
        question_select = page.get_by_label("题目")
        picked = page.get_by_label("学生答案")
        expect(student_select.locator("option")).to_have_count(2)
        expect(question_select.locator("option")).to_have_count(2)
        expect(question_select).not_to_contain_text("分析洋务运动失败原因")
        expect(page.locator(".answer-reference")).to_contain_text("标准答案A")
        expect(page.locator(".answer-reference")).to_contain_text("分值2")
        expect(page.locator(".exam-card-head .tag").filter(has_text="1 条待确认")).to_be_visible()

        question_select.select_option("8")
        expect(page.locator(".answer-reference")).to_contain_text("标准答案1840")
        question_select.select_option("7")

        page.get_by_role("button", name="生成机器建议").click()
        expect(page.locator(".error")).to_have_text("请选择学生、题目并填写学生答案")
        assert page.evaluate("window.__gradeCalls.length") == 0

        picked.fill(" B ")
        page.get_by_role("button", name="生成机器建议").click()
        expect(page.locator(".ok-banner")).to_have_text("机器建议已生成，必须由老师确认后才会计分")
        expect(picked).to_have_value("")
        expect(page.locator(".answer-row")).to_have_count(3)
        expect(page.locator(".exam-card-head .tag").filter(has_text="2 条待确认")).to_be_visible()
        assert page.evaluate("window.__gradeCalls[0]") == {
            "cmd": "exam_answer_suggest",
            "args": {"studentId": 1, "questionId": 7, "picked": " B "},
        }

        row = page.locator(".answer-row").first
        note = row.get_by_placeholder("终审备注（可选）")
        note.fill(" 老师确认 ")
        row.get_by_role("button", name="判为正确").click()
        expect(page.locator(".ok-banner")).to_have_text("老师终审已保存")
        expect(row).to_contain_text("已终审")
        expect(row).to_contain_text("老师结论 正确")

        row.get_by_role("button", name="判为正确").click()
        expect(page.locator(".ok-banner")).to_have_text("结论未变化，未重复累计错题或掌握度")

        note.fill("补充备注")
        row.get_by_role("button", name="判为正确").click()
        expect(page.locator(".ok-banner")).to_have_text("终审备注已更新，结论和派生统计未变化")

        row.get_by_role("button", name="判为错误").click()
        expect(page.locator(".ok-banner")).to_have_text("改判已完成，错题与掌握度已重算")
        expect(row).to_contain_text("老师结论 错误")

        decide_calls = page.evaluate(
            "window.__gradeCalls.filter((item) => item.cmd === 'exam_answer_human_decide')"
        )
        assert decide_calls == [
            {"cmd": "exam_answer_human_decide", "args": {"answerId": 101, "isCorrect": True, "note": "老师确认"}},
            {"cmd": "exam_answer_human_decide", "args": {"answerId": 101, "isCorrect": True, "note": "老师确认"}},
            {"cmd": "exam_answer_human_decide", "args": {"answerId": 101, "isCorrect": True, "note": "补充备注"}},
            {"cmd": "exam_answer_human_decide", "args": {"answerId": 101, "isCorrect": False, "note": "补充备注"}},
        ]
        page.screenshot(path="/tmp/jiaofu-r2-grade-tab-success.png", full_page=True)
        browser.close()


def test_grade_tab_failures(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1200})
        page.add_init_script(GRADE_MOCK_SCRIPT)
        reach_grade_tab(page, f"{base_url}?gradeSuggestFail=1&gradeDecideFail=1")

        picked = page.get_by_label("学生答案")
        picked.fill("C")
        page.get_by_role("button", name="生成机器建议").click()
        expect(page.locator(".error")).to_contain_text("模拟机器建议失败")
        expect(picked).to_have_value("C")
        expect(page.locator(".answer-row")).to_have_count(2)
        expect(page.locator(".ok-banner")).to_have_count(0)

        row = page.locator(".answer-row").first
        note = row.get_by_placeholder("终审备注（可选）")
        note.fill("等待复核")
        row.get_by_role("button", name="判为正确").click()
        expect(page.locator(".error")).to_contain_text("模拟老师终审失败")
        expect(note).to_have_value("等待复核")
        expect(row).to_contain_text("待老师确认")
        expect(row).to_contain_text("老师结论 未确认")
        expect(page.locator(".ok-banner")).to_have_count(0)

        assert page.evaluate("window.__gradeCalls") == [
            {"cmd": "exam_answer_suggest", "args": {"studentId": 1, "questionId": 7, "picked": "C"}},
            {"cmd": "exam_answer_human_decide", "args": {"answerId": 101, "isCorrect": True, "note": "等待复核"}},
        ]
        page.screenshot(path="/tmp/jiaofu-r2-grade-tab-failures.png", full_page=True)
        browser.close()


if __name__ == "__main__":
    test_grade_tab_success("http://127.0.0.1:4173")
    test_grade_tab_failures("http://127.0.0.1:4173")
    print("exam GradeTab UI characterization: PASS")
