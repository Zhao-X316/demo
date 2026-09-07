"""R3-H1-S5 答案资料分析与三类处置生命周期测试。"""

from collections.abc import Callable
from typing import Optional, Tuple

from playwright.sync_api import Browser, Page, expect, sync_playwright

from test_exam_fixed_intake_shared_shell_ui import (
    MOCK_SCRIPT,
    assert_no_authority_writes,
    calls,
    open_exam,
    prepare_with_pasted_answer,
)


ANSWER_SOURCE_LIFECYCLE_OVERRIDE = r"""
const answerLifecycleBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const answerLifecycleScenario = () => params().get("answerLifecycle") || "";
window.__answerLifecycleAnalysisResolvers = [];
window.__answerLifecycleResolutionResolvers = [];

const answerLifecycleAnalysis = (route = "ready_to_confirm") => {
  const review = baseReview(route);
  return {
    run: {
      ai_run_id: review.sourceAiRunId,
      status: "succeeded",
      output: { state: review.sourceState, confidence: 0.98, issue_codes: [] },
      failure: null,
    },
    review,
  };
};

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const active = answerLifecycleScenario();

  if ((active === "analysis_stale" || active === "resolution_stale")
      && cmd === "exam_fixed_intake_prepare") {
    window.__fixedCalls.push({ cmd, args });
    window.__prepareAttempts += 1;
    const result = preparedResult(args.request);
    const batchId = window.__prepareAttempts === 1 ? 101 : 102;
    result.batchId = batchId;
    result.batchPublicId = `batch-${batchId}`;
    return result;
  }

  if (active === "analysis_double" && cmd === "exam_answer_source_analyze") {
    window.__fixedCalls.push({ cmd, args });
    window.__answerAnalyzeAttempts += 1;
    if (window.__answerAnalyzeAttempts === 1) {
      throw new Error("模拟答案整理暂时失败");
    }
    return new Promise((resolve) => {
      window.__answerLifecycleAnalysisResolvers.push(() => resolve(answerLifecycleAnalysis()));
    });
  }

  if (active === "analysis_stale" && cmd === "exam_answer_source_analyze") {
    window.__fixedCalls.push({ cmd, args });
    window.__answerAnalyzeAttempts += 1;
    if (window.__answerAnalyzeAttempts > 1) {
      const analysis = answerLifecycleAnalysis("blocked");
      analysis.run.ai_run_id = 902;
      analysis.review.ingestBatchId = 102;
      analysis.review.sourceAiRunId = 902;
      return analysis;
    }
    return new Promise((resolve) => {
      window.__answerLifecycleAnalysisResolvers.push(() => resolve(answerLifecycleAnalysis()));
    });
  }

  if (active === "resolution_stale" && cmd === "exam_answer_source_analyze") {
    window.__fixedCalls.push({ cmd, args });
    window.__answerAnalyzeAttempts += 1;
    const analysis = answerLifecycleAnalysis();
    if (window.__answerAnalyzeAttempts > 1) {
      analysis.run.ai_run_id = 902;
      analysis.review.ingestBatchId = 102;
      analysis.review.sourceAiRunId = 902;
    }
    return analysis;
  }

  if ((active === "resolution_double" || active === "resolution_stale")
      && cmd === "exam_answer_source_confirm_matches") {
    window.__fixedCalls.push({ cmd, args });
    return new Promise((resolve, reject) => {
      window.__answerLifecycleResolutionResolvers.push({
        resolve: () => resolve(baseReview("confirmed")),
        reject: () => reject(new Error("模拟旧答案确认失败")),
      });
    });
  }

  if (active === "resolution_cross"
      && (cmd === "exam_answer_source_keep_bound"
        || cmd === "exam_answer_source_adopt_new_version")) {
    window.__fixedCalls.push({ cmd, args });
    return new Promise((resolve) => {
      window.__answerLifecycleResolutionResolvers.push({
        resolve: () => resolve(baseReview(cmd === "exam_answer_source_keep_bound"
          ? "kept_bound"
          : "adopted_new_version")),
      });
    });
  }

  return answerLifecycleBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{ANSWER_SOURCE_LIFECYCLE_OVERRIDE}")


def test_analysis_retry_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=main&answerLifecycle=analysis_double")
    prepare_with_pasted_answer(page)
    retry = page.get_by_role("button", name="重试整理答案")
    expect(retry).to_be_visible()
    retry.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__answerAnalyzeAttempts >= 2")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_answer_source_analyze")) == 2
    page.evaluate("window.__answerLifecycleAnalysisResolvers[0]()")
    expect(page.get_by_text("一致 1", exact=True)).to_be_visible()
    assert_no_authority_writes(page)
    page.close()


def test_old_analysis_cannot_repopulate_switched_assessment(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&answerLifecycle=analysis_stale")
    prepare_with_pasted_answer(page)
    page.wait_for_function("window.__answerAnalyzeAttempts === 1")
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("冲突 1", exact=True)).to_be_visible()
    page.evaluate("window.__answerLifecycleAnalysisResolvers[0]()")
    page.wait_for_timeout(150)

    assert page.get_by_text("一致 1", exact=True).count() == 0
    assert page.get_by_text("冲突 1", exact=True).count() == 1
    assert page.get_by_label("批改哪份作业").input_value() == "13"
    assert_no_authority_writes(page)
    page.close()


def test_resolution_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=answer_file&answerLifecycle=resolution_double")
    prepare_with_pasted_answer(page)
    button = page.get_by_role("button", name="确认这些答案与当前作业一致")
    expect(button).to_be_visible()
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__answerLifecycleResolutionResolvers.length === 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_answer_source_confirm_matches")) == 1
    page.evaluate("window.__answerLifecycleResolutionResolvers[0].resolve()")
    expect(page.get_by_text("已确认一致", exact=True)).to_be_visible()
    assert_no_authority_writes(page)
    page.close()


def test_resolution_choices_share_one_owner(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1200})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=conflict&answerLifecycle=resolution_cross")
    prepare_with_pasted_answer(page)
    expect(page.get_by_role("button", name="沿用当前作业答案继续")).to_be_visible()
    expect(page.get_by_role("button", name="采用答案与评分点，另存新版本")).to_be_enabled()
    page.evaluate("""
      const buttons = [...document.querySelectorAll("button")];
      buttons.find((node) => node.textContent.includes("沿用当前作业答案继续")).click();
      buttons.find((node) => node.textContent.includes("采用答案与评分点，另存新版本")).click();
    """)
    page.wait_for_function("window.__answerLifecycleResolutionResolvers.length === 1")
    page.wait_for_timeout(100)

    total = len(calls(page, "exam_answer_source_keep_bound")) + len(
        calls(page, "exam_answer_source_adopt_new_version")
    )
    assert total == 1
    page.evaluate("window.__answerLifecycleResolutionResolvers[0].resolve()")
    assert_no_authority_writes(page)
    page.close()


def test_old_resolution_failure_is_silent_after_scope_switch(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&answerLifecycle=resolution_stale")
    prepare_with_pasted_answer(page)
    page.get_by_role("button", name="确认这些答案与当前作业一致").click()
    page.wait_for_function("window.__answerLifecycleResolutionResolvers.length === 1")
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("一致 1", exact=True)).to_be_visible()
    page.evaluate("window.__answerLifecycleResolutionResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧答案确认失败").count() == 0
    assert page.get_by_text("一致 1", exact=True).count() == 1
    assert page.get_by_label("批改哪份作业").input_value() == "13"
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


def test_answer_source_lifecycle(base_url: str) -> None:
    cases = [
        ("analysis retry same-turn reentry", test_analysis_retry_same_turn_calls_provider_once),
        ("stale analysis completion", test_old_analysis_cannot_repopulate_switched_assessment),
        ("resolution same-turn reentry", test_resolution_same_turn_calls_provider_once),
        ("resolution choices shared owner", test_resolution_choices_share_one_owner),
        ("stale resolution failure", test_old_resolution_failure_is_silent_after_scope_switch),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(f"R3-H1-S5 answer-source lifecycle still RED: {failed_names}")


if __name__ == "__main__":
    test_answer_source_lifecycle("http://127.0.0.1:4173")
    print("fixed intake answer-source lifecycle UI: PASS")
