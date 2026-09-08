from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""固定答题卡主观题整卷发布面板的状态、确认、成功与失败行为。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_m25_rubric_update_ui import MOCK_SCRIPT as M25_BASE_MOCK_SCRIPT


PUBLISH_MOCK_SCRIPT = M25_BASE_MOCK_SCRIPT + r"""
window.__subjectivePublishCalls = [];
window.__subjectivePublishSucceeded = false;
const __baseInvokeForSubjectivePublish = window.__TAURI_INTERNALS__.invoke;

function publishAttempt(overrides) {
  return {
    assessment_id: 1,
    assessment_version_id: 1,
    assessment_title: "洋务运动随堂练习",
    attempt_id: 0,
    attempt_state: "grading",
    active_publication_id: null,
    student_id: 0,
    student_no: "",
    student_name: "",
    item_count: 4,
    observed_count: 4,
    confirmed_count: 0,
    teacher_total_score: 0,
    max_total_score: 10,
    published_total_score: null,
    can_publish: false,
    ...overrides,
  };
}

function subjectivePublishWorkbench() {
  const base = subjectiveWorkbench();
  return {
    rows: base.rows,
    attempts: [
      publishAttempt({
        attempt_id: 101,
        attempt_state: "published",
        active_publication_id: 201,
        student_id: 11,
        student_no: "01",
        student_name: "已发布同学",
        confirmed_count: 4,
        teacher_total_score: 8,
        published_total_score: 7,
      }),
      publishAttempt({
        attempt_id: 102,
        attempt_state: window.__subjectivePublishSucceeded ? "published" : "ready_to_publish",
        active_publication_id: window.__subjectivePublishSucceeded ? 202 : null,
        student_id: 12,
        student_no: "02",
        student_name: "待发布同学",
        confirmed_count: 4,
        teacher_total_score: 9,
        published_total_score: window.__subjectivePublishSucceeded ? 9 : null,
        can_publish: !window.__subjectivePublishSucceeded,
      }),
      publishAttempt({
        attempt_id: 103,
        student_id: 13,
        student_no: "03",
        student_name: "终审中同学",
        confirmed_count: 2,
        teacher_total_score: 4,
      }),
    ],
  };
}

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "exam_answer_sheet_subjective_workbench") {
    return subjectivePublishWorkbench();
  }
  if (cmd === "exam_answer_sheet_subjective_publish_attempt") {
    window.__subjectivePublishCalls.push({ cmd, args: structuredClone(args) });
    if (new URLSearchParams(window.location.search).has("subjectivePublishFail")) {
      throw new Error("PUBLISH_FAILED");
    }
    window.__subjectivePublishSucceeded = true;
    return {
      id: 202,
      attempt_id: args.attemptId,
      revision: 2,
      state: "published",
      total_score: 9,
      published_at: "2026-08-02T11:00:00Z",
    };
  }
  return __baseInvokeForSubjectivePublish(cmd, args);
};
"""


def open_subjective_review(page: Page, url: str) -> None:
    page.add_init_script(PUBLISH_MOCK_SCRIPT)
    page.goto(url)
    page.wait_for_load_state("networkidle")
    navigate_exam(page)
    page.get_by_role("button", name="答题卡主观题").click()
    expect(page.locator(".sech").filter(has_text="整份答题卡发布")).to_be_visible()


def attempt_card(page: Page, student_name: str):
    return page.locator("article.objective-attempt").filter(has_text=student_name)


def assert_initial_states(page: Page) -> None:
    expect(page.get_by_text("所有题型终审完成后才可发布", exact=True)).to_be_visible()

    published = attempt_card(page, "已发布同学")
    expect(published.get_by_text("已发布", exact=True).first).to_be_visible()
    expect(published.get_by_text("8", exact=True)).to_be_visible()
    expect(published.get_by_text("/ 10 分", exact=True)).to_be_visible()
    expect(published.get_by_text("已终审 4 / 4 题", exact=True)).to_be_visible()
    expect(published.get_by_text("当前已发布总分：7", exact=True)).to_be_visible()
    expect(published.get_by_role("button", name="已发布")).to_be_disabled()

    ready = attempt_card(page, "待发布同学")
    expect(ready.get_by_text("待发布", exact=True)).to_be_visible()
    expect(ready.get_by_text("9", exact=True)).to_be_visible()
    expect(ready.get_by_role("button", name="确认发布整份答题卡")).to_be_enabled()

    grading = attempt_card(page, "终审中同学")
    expect(grading.get_by_text("终审中", exact=True)).to_be_visible()
    expect(grading.get_by_text("已终审 2 / 4 题", exact=True)).to_be_visible()
    expect(grading.get_by_role("button", name="完成全部终审后发布")).to_be_disabled()


def test_cancel_and_publish_success(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1100})
        open_subjective_review(page, base_url)
        assert_initial_states(page)

        ready_button = attempt_card(page, "待发布同学").get_by_role(
            "button", name="确认发布整份答题卡"
        )
        cancelled_dialogs: list[str] = []

        def dismiss_dialog(dialog) -> None:
            cancelled_dialogs.append(dialog.message)
            dialog.dismiss()

        page.once("dialog", dismiss_dialog)
        ready_button.click()
        assert cancelled_dialogs == [
            "确认发布 待发布同学 的本次答题卡成绩？只采用当前老师终审 revision。"
        ]
        assert page.evaluate("window.__subjectivePublishCalls.length") == 0
        expect(ready_button).to_be_enabled()

        accepted_dialogs: list[str] = []

        def accept_dialog(dialog) -> None:
            accepted_dialogs.append(dialog.message)
            dialog.accept()

        page.once("dialog", accept_dialog)
        ready_button.click()
        expect(
            page.get_by_text(
                "待发布同学 的答题卡成绩已发布：9 分（revision 2）", exact=True
            )
        ).to_be_visible()
        assert accepted_dialogs == cancelled_dialogs
        assert page.evaluate("window.__subjectivePublishCalls") == [
            {
                "cmd": "exam_answer_sheet_subjective_publish_attempt",
                "args": {"attemptId": 102},
            }
        ]

        refreshed = attempt_card(page, "待发布同学")
        expect(refreshed.get_by_text("已发布", exact=True).first).to_be_visible()
        expect(refreshed.get_by_text("当前已发布总分：9", exact=True)).to_be_visible()
        expect(refreshed.get_by_role("button", name="已发布")).to_be_disabled()
        page.screenshot(
            path="/tmp/jiaofu-r2-subjective-attempt-publish-success.png", full_page=True
        )
        browser.close()


def test_publish_failure(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1100})
        open_subjective_review(page, f"{base_url}?subjectivePublishFail=1")

        ready = attempt_card(page, "待发布同学")
        page.once("dialog", lambda dialog: dialog.accept())
        ready.get_by_role("button", name="确认发布整份答题卡").click()
        expect(page.get_by_text("Error: PUBLISH_FAILED", exact=True)).to_be_visible()
        expect(ready.get_by_text("待发布", exact=True)).to_be_visible()
        expect(ready.get_by_role("button", name="确认发布整份答题卡")).to_be_enabled()
        expect(page.get_by_text("待发布同学 的答题卡成绩已发布", exact=False)).to_have_count(0)
        assert page.evaluate("window.__subjectivePublishCalls") == [
            {
                "cmd": "exam_answer_sheet_subjective_publish_attempt",
                "args": {"attemptId": 102},
            }
        ]
        page.screenshot(
            path="/tmp/jiaofu-r2-subjective-attempt-publish-failure.png", full_page=True
        )
        browser.close()


if __name__ == "__main__":
    test_cancel_and_publish_success("http://127.0.0.1:4173/")
    test_publish_failure("http://127.0.0.1:4173/")
    print("exam subjective attempt publish panel UI characterization: PASS")
