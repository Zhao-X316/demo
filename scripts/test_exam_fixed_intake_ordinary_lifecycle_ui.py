from ui_navigation import expand_new_intake
"""R3-H1-S8 普通卷分析与确认生命周期测试。"""

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


ORDINARY_LIFECYCLE_OVERRIDE = r"""
const ordinaryBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const ordinaryLifecycleScenario = () => params().get("ordinaryLifecycle") || "";
window.__ordinaryAnalyzeResolvers = [];
window.__ordinaryConfirmResolvers = [];
window.__ordinarySyncResolvers = [];
window.__ordinaryRegionResolvers = [];

const ordinaryReadyRun = (pageId, aiRunId = 8000 + pageId) => ({
  ai_run_id: aiRunId,
  status: "succeeded",
  output: {
    schema_version: 1,
    page_id: pageId,
    expected_page_no: pageId - 300,
    state: "ready",
    quality: { result: "pass", issue_codes: [] },
    alignment: { confidence: 0.99 },
    regions: [{
      assessment_item_id: 501,
      region_index: 0,
      mapping_confidence: 0.99,
      mark_cells: [{ label: "A" }, { label: "B" }],
    }],
    printed_questions: [],
    confidence: 0.99,
    issue_codes: [],
  },
  failure: null,
});

const ordinaryFailedRun = (pageId) => ({
  ai_run_id: 8000 + pageId,
  status: "failed",
  output: null,
  failure: {
    code: "SIMULATED_RETRYABLE",
    safe_message: `第${pageId - 300}页可重试`,
    retryable: true,
  },
});

const ordinaryConfirmation = (pageId, aiRunId) => ({
  confirmation: {
    id: 6000 + pageId,
    ai_run_id: aiRunId,
    page_id: pageId,
    alignment_revision_id: 6100 + pageId,
    region_revision_ids: [7000 + pageId],
    confirmed_by: "local_teacher",
    created_at: "2026-08-02T16:20:00Z",
  },
  alignment: { id: 6100 + pageId, decision: "teacher_confirmed" },
  regions: [{
    id: 7000 + pageId,
    assessment_item_id: 501,
    region_index: 0,
    decision: "teacher_confirmed",
  }],
});

const ordinarySyncSummary = (pageId, aiRunId) => ({
  schema_version: 1,
  state: "completed",
  assessment_version_id: 12,
  page_no: pageId - 300,
  source_page_id: pageId,
  source_ai_run_id: aiRunId,
  printed_question_count: 0,
  eligible_count: 0,
  enqueued_count: 0,
  matched_count: 0,
  candidate_created_count: 0,
  needs_review_count: 0,
  privacy_rejected_count: 0,
  low_confidence_skipped_count: 0,
  failed_count: 0,
  reused_existing_source: false,
});

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const active = ordinaryLifecycleScenario();

  if (cmd === "exam_ordinary_paper_analyze_page") {
    window.__fixedCalls.push({ cmd, args });
    const isRetry = String(args.idempotencyKey || "").includes(":retry:");
    if (active === "analyze_retry_double") {
      if (!isRetry) return ordinaryFailedRun(args.pageId);
      return new Promise((resolve, reject) => {
        window.__ordinaryAnalyzeResolvers.push({
          resolve: () => resolve(ordinaryReadyRun(args.pageId, 9000 + args.pageId)),
          reject: () => reject(new Error("模拟重试失败")),
        });
      });
    }
    if (active.startsWith("analyze_stale_") && window.__ordinaryAnalyzeResolvers.length === 0) {
      return new Promise((resolve, reject) => {
        window.__ordinaryAnalyzeResolvers.push({
          resolve: () => resolve(ordinaryReadyRun(args.pageId)),
          reject: () => reject(new Error("模拟旧普通卷分析失败")),
        });
      });
    }
    return ordinaryReadyRun(args.pageId);
  }

  if (cmd === "exam_ordinary_paper_confirm_page_structure") {
    window.__fixedCalls.push({ cmd, args });
    if (active === "confirm_double"
        || (active.startsWith("confirm_stale_") && window.__ordinaryConfirmResolvers.length === 0)) {
      return new Promise((resolve, reject) => {
        window.__ordinaryConfirmResolvers.push({
          resolve: () => resolve(ordinaryConfirmation(args.pageId, args.aiRunId)),
          reject: () => reject(new Error("模拟旧普通卷确认失败")),
        });
      });
    }
    return ordinaryConfirmation(args.pageId, args.aiRunId);
  }

  if (cmd === "exam_ordinary_paper_sync_questions") {
    window.__fixedCalls.push({ cmd, args });
    if (active.startsWith("sync_stale_") && window.__ordinarySyncResolvers.length === 0) {
      return new Promise((resolve, reject) => {
        window.__ordinarySyncResolvers.push({
          resolve: () => resolve(ordinarySyncSummary(args.pageId, args.aiRunId)),
          reject: () => reject(new Error("模拟旧普通卷题库同步失败")),
        });
      });
    }
    return ordinarySyncSummary(args.pageId, args.aiRunId);
  }

  if (cmd === "exam_objective_recognize_region") {
    window.__fixedCalls.push({ cmd, args });
    if (active.startsWith("region_stale_") && window.__ordinaryRegionResolvers.length === 0) {
      return new Promise((resolve, reject) => {
        window.__ordinaryRegionResolvers.push({
          resolve: () => resolve({ status: "queued" }),
          reject: () => reject(new Error("模拟旧普通卷题区识别失败")),
        });
      });
    }
    return { status: "queued" };
  }

  return ordinaryBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{ORDINARY_LIFECYCLE_OVERRIDE}")


def prepare_without_answer(page: Page) -> None:
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()


def switch_assessment_and_prepare(page: Page) -> None:
    expand_new_intake(page)
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()


def reach_ordinary_processing(page: Page) -> None:
    prepare_without_answer(page)
    page.get_by_role("button", name="普通试卷").click()
    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认这 3 份学生顺序").click()
    expect(page.get_by_text("看一眼照片是否清楚", exact=True)).to_be_visible()
    page.get_by_role("button", name="照片都清楚，确认并建立归属").click()
    expect(page.get_by_text("普通试卷正在自动识别", exact=True)).to_be_visible()


def reach_retryable_analysis(page: Page) -> None:
    reach_ordinary_processing(page)
    expect(page.get_by_role("button", name="重试可恢复页面")).to_be_visible()


def reach_ready_confirmation(page: Page) -> None:
    reach_ordinary_processing(page)
    expect(page.get_by_role("button", name="确认 6 页并开始批改")).to_be_visible()


def test_analysis_retry_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&ordinaryLifecycle=analyze_retry_double")
    reach_retryable_analysis(page)
    button = page.get_by_role("button", name="重试可恢复页面")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__ordinaryAnalyzeResolvers.length >= 1")
    page.wait_for_timeout(100)

    retry_calls = [
        item for item in calls(page, "exam_ordinary_paper_analyze_page")
        if ":retry:" in item["args"]["idempotencyKey"]
    ]
    assert len(retry_calls) == 1
    assert_no_authority_writes(page)
    page.close()


def settle_stale_analysis(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&ordinaryLifecycle=analyze_stale_{outcome}")
    reach_ordinary_processing(page)
    page.wait_for_function("window.__ordinaryAnalyzeResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate(f"window.__ordinaryAnalyzeResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_ordinary_paper_analyze_page")) == 1
    assert page.locator(".error").filter(has_text="模拟旧普通卷分析失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_analysis_stale_success_stops_old_loop(browser: Browser, base_url: str) -> None:
    settle_stale_analysis(browser, base_url, "resolve")


def test_analysis_stale_failure_is_silent_and_stops_old_loop(browser: Browser, base_url: str) -> None:
    settle_stale_analysis(browser, base_url, "reject")


def test_confirmation_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&ordinaryLifecycle=confirm_double")
    reach_ready_confirmation(page)
    button = page.get_by_role("button", name="确认 6 页并开始批改")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__ordinaryConfirmResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_ordinary_paper_confirm_page_structure")) == 1
    assert_no_authority_writes(page)
    page.close()


def settle_stale_confirmation(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&ordinaryLifecycle=confirm_stale_{outcome}")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认 6 页并开始批改").click()
    page.wait_for_function("window.__ordinaryConfirmResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate(f"window.__ordinaryConfirmResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_ordinary_paper_confirm_page_structure")) == 1
    assert calls(page, "exam_ordinary_paper_sync_questions") == []
    assert calls(page, "exam_objective_recognize_region") == []
    assert page.locator(".error").filter(has_text="模拟旧普通卷确认失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_confirmation_stale_success_stops_before_sync(browser: Browser, base_url: str) -> None:
    settle_stale_confirmation(browser, base_url, "resolve")


def test_confirmation_stale_failure_is_silent(browser: Browser, base_url: str) -> None:
    settle_stale_confirmation(browser, base_url, "reject")


def settle_stale_sync(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&ordinaryLifecycle=sync_stale_{outcome}")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认 6 页并开始批改").click()
    page.wait_for_function("window.__ordinarySyncResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate(f"window.__ordinarySyncResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_ordinary_paper_confirm_page_structure")) == 1
    assert len(calls(page, "exam_ordinary_paper_sync_questions")) == 1
    assert calls(page, "exam_objective_recognize_region") == []
    assert page.locator(".error").filter(has_text="模拟旧普通卷题库同步失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_sync_stale_success_stops_before_regions(browser: Browser, base_url: str) -> None:
    settle_stale_sync(browser, base_url, "resolve")


def test_sync_stale_failure_is_silent_and_stops_regions(browser: Browser, base_url: str) -> None:
    settle_stale_sync(browser, base_url, "reject")


def settle_stale_region(browser: Browser, base_url: str, outcome: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&ordinaryLifecycle=region_stale_{outcome}")
    reach_ready_confirmation(page)
    page.get_by_role("button", name="确认 6 页并开始批改").click()
    page.wait_for_function("window.__ordinaryRegionResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate(f"window.__ordinaryRegionResolvers[0].{outcome}()")
    page.wait_for_timeout(180)

    assert len(calls(page, "exam_ordinary_paper_confirm_page_structure")) == 1
    assert len(calls(page, "exam_ordinary_paper_sync_questions")) == 1
    assert len(calls(page, "exam_objective_recognize_region")) == 1
    assert page.locator(".error").filter(has_text="模拟旧普通卷题区识别失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_region_stale_success_stops_old_pages(browser: Browser, base_url: str) -> None:
    settle_stale_region(browser, base_url, "resolve")


def test_region_stale_failure_is_silent_and_stops_old_pages(browser: Browser, base_url: str) -> None:
    settle_stale_region(browser, base_url, "reject")


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


def test_ordinary_lifecycle(base_url: str) -> None:
    cases = [
        ("analysis retry same-turn reentry", test_analysis_retry_same_turn_calls_provider_once),
        ("stale analysis success", test_analysis_stale_success_stops_old_loop),
        ("stale analysis failure", test_analysis_stale_failure_is_silent_and_stops_old_loop),
        ("confirmation same-turn reentry", test_confirmation_same_turn_calls_provider_once),
        ("stale confirmation success", test_confirmation_stale_success_stops_before_sync),
        ("stale confirmation failure", test_confirmation_stale_failure_is_silent),
        ("stale sync success", test_sync_stale_success_stops_before_regions),
        ("stale sync failure", test_sync_stale_failure_is_silent_and_stops_regions),
        ("stale region success", test_region_stale_success_stops_old_pages),
        ("stale region failure", test_region_stale_failure_is_silent_and_stops_old_pages),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(f"R3-H1-S8 ordinary lifecycle still RED: {failed_names}")


if __name__ == "__main__":
    test_ordinary_lifecycle("http://127.0.0.1:4173")
    print("fixed intake ordinary lifecycle UI: PASS")
