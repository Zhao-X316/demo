from ui_navigation import expand_new_intake
"""R3-H1-S9 答题卡模板分析与确认生命周期测试。"""

from collections.abc import Callable
from typing import Optional, Tuple

from playwright.sync_api import Browser, Page, expect, sync_playwright

from test_exam_fixed_intake_shared_shell_ui import (
    MOCK_SCRIPT,
    assert_no_authority_writes,
    calls,
    open_exam,
    select_student_papers,
)


ANSWER_SHEET_TEMPLATE_LIFECYCLE_OVERRIDE = r"""
const answerSheetTemplateBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const answerSheetTemplateLifecycleScenario = () => params().get("answerSheetLifecycle") || "";
window.__answerSheetTemplateDialogResolvers = [];
window.__answerSheetTemplateAnalyzeResolvers = [];
window.__answerSheetTemplateConfirmResolvers = [];
window.__answerSheetTemplateRefreshResolvers = [];
window.__answerSheetTemplateStatusCalls = 0;

const answerSheetTemplateStatus = (ready = false, assessmentVersionId = 12) => ({
  assessmentVersionId,
  pageNo: 1,
  activeTemplate: ready ? {
    id: 801,
    public_id: "answer-sheet-template-801",
    assessment_version_id: assessmentVersionId,
    revision: 2,
    template_version: "answer-sheet-v2",
    page_no: 1,
    blank_artifact_id: 802,
    source_ai_run_id: 9301,
    confirmed_by: "local_teacher",
    state: "active",
    created_at: "2026-08-02T18:30:00Z",
  } : null,
  templateSet: {
    assessment_version_id: assessmentVersionId,
    template_version: "answer-sheet-v2",
    ready,
    template_set_hash: ready ? "answer-sheet-template-set-v2" : null,
    pages: [{
      page_no: 1,
      expected_item_count: 2,
      objective_item_count: 1,
      subjective_item_count: 1,
      active_template_revision_id: ready ? 801 : null,
      ready,
      issue_codes: [],
    }],
    issue_codes: [],
  },
});

const answerSheetTemplateRun = (aiRunId = 9301) => ({
  ai_run_id: aiRunId,
  status: "succeeded",
  output: {
    state: "ready",
    page_no: 1,
    canvas_width: 1000,
    canvas_height: 1400,
    alignment_mode: "printed_anchors",
    anchors: [],
    items: [],
    subjective_regions: [],
    confidence: 0.99,
    issue_codes: [],
  },
  failure: null,
});

const answerSheetTemplateRevision = (assessmentVersionId = 12) => ({
  id: 801,
  public_id: "answer-sheet-template-801",
  assessment_version_id: assessmentVersionId,
  revision: 2,
  template_version: "answer-sheet-v2",
  page_no: 1,
  blank_artifact_id: 802,
  source_ai_run_id: 9301,
  confirmed_by: "local_teacher",
  state: "active",
  created_at: "2026-08-02T18:30:00Z",
});

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const active = answerSheetTemplateLifecycleScenario();

  if (cmd === "plugin:dialog|open"
      && active === "dialog_stale"
      && window.__dialogCount >= 1) {
    window.__fixedCalls.push({ cmd, args });
    window.__dialogCount += 1;
    return new Promise((resolve) => {
      window.__answerSheetTemplateDialogResolvers.push(
        () => resolve("/tmp/BLANK_0001.jpg"),
      );
    });
  }

  if (cmd === "exam_answer_sheet_template_status") {
    window.__fixedCalls.push({ cmd, args });
    window.__answerSheetTemplateStatusCalls += 1;
    const callNo = window.__answerSheetTemplateStatusCalls;
    if (active.startsWith("refresh_stale_") && callNo === 2) {
      return new Promise((resolve, reject) => {
        window.__answerSheetTemplateRefreshResolvers.push({
          resolve: () => resolve(answerSheetTemplateStatus(true)),
          reject: () => reject(new Error("模拟旧答题卡模板状态刷新失败")),
        });
      });
    }
    if (active === "current_chain" && callNo === 2) {
      return answerSheetTemplateStatus(true);
    }
    return answerSheetTemplateStatus(false, args.pageId === 401 ? 13 : 12);
  }

  if (cmd === "exam_answer_sheet_analyze_template") {
    window.__fixedCalls.push({ cmd, args });
    if (active === "analyze_double"
        || (active.startsWith("analyze_stale_")
            && window.__answerSheetTemplateAnalyzeResolvers.length === 0)) {
      return new Promise((resolve, reject) => {
        window.__answerSheetTemplateAnalyzeResolvers.push({
          resolve: () => resolve(answerSheetTemplateRun()),
          reject: () => reject(new Error("模拟旧答题卡模板分析失败")),
        });
      });
    }
    return answerSheetTemplateRun();
  }

  if (cmd === "exam_answer_sheet_confirm_template") {
    window.__fixedCalls.push({ cmd, args });
    if (active === "confirm_double"
        || (active.startsWith("confirm_stale_")
            && window.__answerSheetTemplateConfirmResolvers.length === 0)) {
      return new Promise((resolve, reject) => {
        window.__answerSheetTemplateConfirmResolvers.push({
          resolve: () => resolve(answerSheetTemplateRevision()),
          reject: () => reject(new Error("模拟旧答题卡模板确认失败")),
        });
      });
    }
    return answerSheetTemplateRevision();
  }

  return answerSheetTemplateBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{ANSWER_SHEET_TEMPLATE_LIFECYCLE_OVERRIDE}")


def prepare_answer_sheet(page: Page) -> None:
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_role("button", name="选择第 1 页空白卡")).to_be_visible()


def switch_assessment_and_prepare(page: Page) -> None:
    expand_new_intake(page)
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_role("button", name="选择第 1 页空白卡")).to_be_visible()


def reach_ready_confirmation(page: Page) -> None:
    prepare_answer_sheet(page)
    page.get_by_role("button", name="选择第 1 页空白卡").click()
    expect(page.get_by_role("button", name="确认第 1 页，开始处理 1 张学生卡")).to_be_visible()


def test_analysis_same_turn_calls_dialog_and_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=analyze_double")
    prepare_answer_sheet(page)
    button = page.get_by_role("button", name="选择第 1 页空白卡")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__answerSheetTemplateAnalyzeResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert page.evaluate("window.__dialogCount") == 2
    assert len(calls(page, "exam_answer_sheet_analyze_template")) == 1
    assert_no_authority_writes(page)
    page.close()


def test_stale_dialog_return_does_not_analyze(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=dialog_stale")
    prepare_answer_sheet(page)
    page.get_by_role("button", name="选择第 1 页空白卡").click()
    page.wait_for_function("window.__answerSheetTemplateDialogResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__answerSheetTemplateDialogResolvers[0]()")
    page.wait_for_timeout(180)

    assert calls(page, "exam_answer_sheet_analyze_template") == []
    assert_no_authority_writes(page)
    page.close()


def settle_stale_analysis(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=analyze_stale_{outcome}")
    prepare_answer_sheet(page)
    page.get_by_role("button", name="选择第 1 页空白卡").click()
    page.wait_for_function("window.__answerSheetTemplateAnalyzeResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate(f"window.__answerSheetTemplateAnalyzeResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert page.get_by_role("button", name="确认第 1 页，开始处理 1 张学生卡").count() == 0
    assert page.locator(".error").filter(has_text="模拟旧答题卡模板分析失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_stale_analysis_success_is_ignored(browser: Browser, base_url: str) -> None:
    settle_stale_analysis(browser, base_url, "resolve")


def test_stale_analysis_failure_is_silent(browser: Browser, base_url: str) -> None:
    settle_stale_analysis(browser, base_url, "reject")


def test_confirmation_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=confirm_double")
    reach_ready_confirmation(page)
    button = page.get_by_role("button", name="确认第 1 页，开始处理 1 张学生卡")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__answerSheetTemplateConfirmResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_answer_sheet_confirm_template")) == 1
    assert_no_authority_writes(page)
    page.close()


def settle_stale_confirmation(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=confirm_stale_{outcome}")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认第 1 页，开始处理 1 张学生卡").click()
    page.wait_for_function("window.__answerSheetTemplateConfirmResolvers.length === 1")
    switch_assessment_and_prepare(page)
    refresh_count = len(calls(page, "exam_answer_sheet_template_status"))
    page.evaluate(f"window.__answerSheetTemplateConfirmResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_answer_sheet_template_status")) == refresh_count
    assert calls(page, "exam_answer_sheet_process_page") == []
    assert page.locator(".error").filter(has_text="模拟旧答题卡模板确认失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_stale_confirmation_success_stops_before_refresh(browser: Browser, base_url: str) -> None:
    settle_stale_confirmation(browser, base_url, "resolve")


def test_stale_confirmation_failure_is_silent(browser: Browser, base_url: str) -> None:
    settle_stale_confirmation(browser, base_url, "reject")


def settle_stale_refresh(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=refresh_stale_{outcome}")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认第 1 页，开始处理 1 张学生卡").click()
    page.wait_for_function("window.__answerSheetTemplateRefreshResolvers.length === 1")
    switch_assessment_and_prepare(page)
    process_count = len(calls(page, "exam_answer_sheet_process_page"))
    page.evaluate(f"window.__answerSheetTemplateRefreshResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_answer_sheet_process_page")) == process_count
    assert page.locator(".error").filter(has_text="模拟旧答题卡模板状态刷新失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_stale_refresh_success_stops_before_page_processing(browser: Browser, base_url: str) -> None:
    settle_stale_refresh(browser, base_url, "resolve")


def test_stale_refresh_failure_is_silent(browser: Browser, base_url: str) -> None:
    settle_stale_refresh(browser, base_url, "reject")


def test_current_chain_refreshes_then_processes_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=answer_sheet&answerSheetLifecycle=current_chain")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认第 1 页，开始处理 1 张学生卡").click()
    page.wait_for_function(
        "window.__fixedCalls.filter(item => item.cmd === 'exam_answer_sheet_process_page').length === 1",
    )

    commands = page.evaluate(
        "window.__fixedCalls.map(item => item.cmd).filter(cmd => "
        "['exam_answer_sheet_confirm_template', 'exam_answer_sheet_template_status', "
        "'exam_answer_sheet_process_page'].includes(cmd))",
    )
    assert commands[-3:] == [
        "exam_answer_sheet_confirm_template",
        "exam_answer_sheet_template_status",
        "exam_answer_sheet_process_page",
    ]
    assert len(calls(page, "exam_answer_sheet_confirm_template")) == 1
    assert len(calls(page, "exam_answer_sheet_process_page")) == 1
    assert_no_authority_writes(page)
    page.close()


def run_case(
    browser: Browser,
    base_url: str,
    name: str,
    case: Callable[[Browser, str], None],
) -> Tuple[str, Optional[str]]:
    try:
        case(browser, base_url)
        print(f"PASS {name}")
        return name, None
    except Exception as error:  # noqa: BLE001 - aggregate independent lifecycle cases
        print(f"RED  {name}: {error}")
        return name, str(error)


def test_answer_sheet_template_lifecycle(base_url: str) -> None:
    cases = [
        ("analysis same-turn reentry", test_analysis_same_turn_calls_dialog_and_provider_once),
        ("stale blank-card dialog", test_stale_dialog_return_does_not_analyze),
        ("stale analysis success", test_stale_analysis_success_is_ignored),
        ("stale analysis failure", test_stale_analysis_failure_is_silent),
        ("confirmation same-turn reentry", test_confirmation_same_turn_calls_provider_once),
        ("stale confirmation success", test_stale_confirmation_success_stops_before_refresh),
        ("stale confirmation failure", test_stale_confirmation_failure_is_silent),
        ("stale refresh success", test_stale_refresh_success_stops_before_page_processing),
        ("stale refresh failure", test_stale_refresh_failure_is_silent),
        ("current confirmation chain", test_current_chain_refreshes_then_processes_once),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(
            f"R3-H1-S9 answer-sheet template lifecycle still RED: {failed_names}",
        )


if __name__ == "__main__":
    test_answer_sheet_template_lifecycle("http://127.0.0.1:4173")
    print("fixed intake answer-sheet template lifecycle UI: PASS")
