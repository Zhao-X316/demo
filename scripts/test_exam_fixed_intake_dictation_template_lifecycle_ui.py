from ui_navigation import expand_new_intake
"""R3-H1-S10 默写模板分析与确认生命周期测试。"""

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


DICTATION_TEMPLATE_LIFECYCLE_OVERRIDE = r"""
const dictationTemplateBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const dictationTemplateLifecycleScenario = () => params().get("dictationTemplateLifecycle") || "";
window.__dictationTemplateDialogResolvers = [];
window.__dictationTemplateAnalyzeResolvers = [];
window.__dictationTemplateConfirmResolvers = [];

const dictationTemplateStatus = (ready = false, assessmentVersionId = 12) => ({
  assessmentVersionId,
  pageNo: 1,
  activeTemplate: ready ? {
    id: 1001,
    assessment_version_id: assessmentVersionId,
    revision: 3,
    template_version: "dictation-v3",
    page_no: 1,
    source_ai_run_id: 9401,
    state: "active",
  } : null,
});

const dictationTemplateRun = (aiRunId = 9401) => ({
  ai_run_id: aiRunId,
  status: "succeeded",
  output: {
    state: "ready",
    page_no: 1,
    canvas_width: 1000,
    canvas_height: 1400,
    regions: [],
    confidence: 0.99,
    issue_codes: [],
  },
  failure: null,
});

const dictationTemplateRevision = (assessmentVersionId = 12) => ({
  id: 1001,
  assessment_version_id: assessmentVersionId,
  revision: 3,
  template_version: "dictation-v3",
  page_no: 1,
  source_ai_run_id: 9401,
  state: "active",
});

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const active = dictationTemplateLifecycleScenario();

  if (cmd === "plugin:dialog|open"
      && active === "dialog_stale"
      && window.__dialogCount >= 1) {
    window.__fixedCalls.push({ cmd, args });
    window.__dialogCount += 1;
    return new Promise((resolve) => {
      window.__dictationTemplateDialogResolvers.push(
        () => resolve("/tmp/DICTATION_BLANK_0001.jpg"),
      );
    });
  }

  if (cmd === "exam_dictation_template_status") {
    window.__fixedCalls.push({ cmd, args });
    return dictationTemplateStatus(false, args.referencePageId === 401 ? 13 : 12);
  }

  if (cmd === "exam_dictation_analyze_template") {
    window.__fixedCalls.push({ cmd, args });
    if (active === "analyze_double"
        || (active.startsWith("analyze_stale_")
            && window.__dictationTemplateAnalyzeResolvers.length === 0)) {
      return new Promise((resolve, reject) => {
        window.__dictationTemplateAnalyzeResolvers.push({
          resolve: () => resolve(dictationTemplateRun()),
          reject: () => reject(new Error("模拟旧默写模板分析失败")),
        });
      });
    }
    return dictationTemplateRun();
  }

  if (cmd === "exam_dictation_confirm_template") {
    window.__fixedCalls.push({ cmd, args });
    if (active === "confirm_double"
        || (active.startsWith("confirm_stale_")
            && window.__dictationTemplateConfirmResolvers.length === 0)) {
      return new Promise((resolve, reject) => {
        window.__dictationTemplateConfirmResolvers.push({
          resolve: () => resolve({ template: dictationTemplateRevision() }),
          reject: () => reject(new Error("模拟旧默写模板确认失败")),
        });
      });
    }
    return { template: dictationTemplateRevision() };
  }

  return dictationTemplateBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{DICTATION_TEMPLATE_LIFECYCLE_OVERRIDE}")


def prepare_dictation(page: Page) -> None:
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_role("button", name="选择一张默写空白页")).to_be_visible()


def switch_assessment_and_prepare(page: Page) -> None:
    expand_new_intake(page)
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_role("button", name="选择一张默写空白页")).to_be_visible()


def reach_ready_confirmation(page: Page) -> None:
    prepare_dictation(page)
    page.get_by_role("button", name="选择一张默写空白页").click()
    expect(page.get_by_role("button", name="确认这张空白页，识别 1 份默写")).to_be_visible()


def test_analysis_same_turn_calls_dialog_and_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=dictation&dictationTemplateLifecycle=analyze_double")
    prepare_dictation(page)
    button = page.get_by_role("button", name="选择一张默写空白页")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__dictationTemplateAnalyzeResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert page.evaluate("window.__dialogCount") == 2
    assert len(calls(page, "exam_dictation_analyze_template")) == 1
    assert_no_authority_writes(page)
    page.close()


def test_stale_dialog_return_does_not_analyze(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=dictation&dictationTemplateLifecycle=dialog_stale")
    prepare_dictation(page)
    page.get_by_role("button", name="选择一张默写空白页").click()
    page.wait_for_function("window.__dictationTemplateDialogResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__dictationTemplateDialogResolvers[0]()")
    page.wait_for_timeout(180)

    assert calls(page, "exam_dictation_analyze_template") == []
    assert_no_authority_writes(page)
    page.close()


def settle_stale_analysis(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=dictation&dictationTemplateLifecycle=analyze_stale_{outcome}")
    prepare_dictation(page)
    page.get_by_role("button", name="选择一张默写空白页").click()
    page.wait_for_function("window.__dictationTemplateAnalyzeResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate(f"window.__dictationTemplateAnalyzeResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert page.get_by_role("button", name="确认这张空白页，识别 1 份默写").count() == 0
    assert page.locator(".error").filter(has_text="模拟旧默写模板分析失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_stale_analysis_success_is_ignored(browser: Browser, base_url: str) -> None:
    settle_stale_analysis(browser, base_url, "resolve")


def test_stale_analysis_failure_is_silent(browser: Browser, base_url: str) -> None:
    settle_stale_analysis(browser, base_url, "reject")


def test_confirmation_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=dictation&dictationTemplateLifecycle=confirm_double")
    reach_ready_confirmation(page)
    button = page.get_by_role("button", name="确认这张空白页，识别 1 份默写")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__dictationTemplateConfirmResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_dictation_confirm_template")) == 1
    assert_no_authority_writes(page)
    page.close()


def settle_stale_confirmation(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=dictation&dictationTemplateLifecycle=confirm_stale_{outcome}")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认这张空白页，识别 1 份默写").click()
    page.wait_for_function("window.__dictationTemplateConfirmResolvers.length === 1")
    switch_assessment_and_prepare(page)
    process_count = len(calls(page, "exam_dictation_process_page"))
    page.evaluate(f"window.__dictationTemplateConfirmResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_dictation_process_page")) == process_count
    assert page.locator(".error").filter(has_text="模拟旧默写模板确认失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_stale_confirmation_success_stops_before_page_processing(browser: Browser, base_url: str) -> None:
    settle_stale_confirmation(browser, base_url, "resolve")


def test_stale_confirmation_failure_is_silent(browser: Browser, base_url: str) -> None:
    settle_stale_confirmation(browser, base_url, "reject")


def test_current_chain_confirms_then_processes_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&material=dictation&dictationTemplateLifecycle=current_chain")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认这张空白页，识别 1 份默写").click()
    page.wait_for_function(
        "window.__fixedCalls.filter(item => item.cmd === 'exam_dictation_process_page').length === 1",
    )

    commands = page.evaluate(
        "window.__fixedCalls.map(item => item.cmd).filter(cmd => "
        "['exam_dictation_confirm_template', 'exam_dictation_process_page'].includes(cmd))",
    )
    assert commands[-2:] == [
        "exam_dictation_confirm_template",
        "exam_dictation_process_page",
    ]
    assert len(calls(page, "exam_dictation_confirm_template")) == 1
    assert len(calls(page, "exam_dictation_process_page")) == 1
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


def test_dictation_template_lifecycle(base_url: str) -> None:
    cases = [
        ("analysis same-turn reentry", test_analysis_same_turn_calls_dialog_and_provider_once),
        ("stale blank-page dialog", test_stale_dialog_return_does_not_analyze),
        ("stale analysis success", test_stale_analysis_success_is_ignored),
        ("stale analysis failure", test_stale_analysis_failure_is_silent),
        ("confirmation same-turn reentry", test_confirmation_same_turn_calls_provider_once),
        ("stale confirmation success", test_stale_confirmation_success_stops_before_page_processing),
        ("stale confirmation failure", test_stale_confirmation_failure_is_silent),
        ("current confirmation chain", test_current_chain_confirms_then_processes_once),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(
            f"R3-H1-S10 dictation template lifecycle still RED: {failed_names}",
        )


if __name__ == "__main__":
    test_dictation_template_lifecycle("http://127.0.0.1:4173")
    print("fixed intake dictation template lifecycle UI: PASS")
