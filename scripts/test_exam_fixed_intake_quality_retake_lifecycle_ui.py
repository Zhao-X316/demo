"""R3-H1-S7 页面质量确认与单页重拍生命周期测试。"""

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


QUALITY_RETAKE_LIFECYCLE_OVERRIDE = r"""
const qualityRetakeBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const qualityRetakeScenario = () => params().get("qualityRetake") || "";
window.__qualityLifecycleResolvers = [];
window.__qualityEvidenceResolvers = [];
window.__retakeDialogResolvers = [];
window.__retakeLifecycleResolvers = [];
window.__retakeEvidenceResolvers = [];
window.__qualityLifecycleConfirmed = false;
window.__retakeLifecycleSucceeded = false;

const qualityConfirmation = (rejectedPageIds) => ({
  qualityReviewCompleted: true,
  mappedGroupCount: rejectedPageIds.length ? 2 : 3,
  rejectedGroupCount: rejectedPageIds.length ? 1 : 0,
  nextAction: rejectedPageIds.length ? "重拍异常页面" : "进入对应材料识别",
});

const retakeReplacement = () => ({
  replacementPageId: 1302,
  activatedStudent: true,
  mappedGroupCount: 3,
  rejectedGroupCount: 0,
  nextAction: "进入批改终审",
});

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const active = qualityRetakeScenario();

  if (active.includes("stale") && cmd === "exam_fixed_intake_prepare") {
    window.__fixedCalls.push({ cmd, args });
    window.__prepareAttempts += 1;
    const result = preparedResult(args.request);
    const batchId = window.__prepareAttempts === 1 ? 101 : 102;
    result.batchId = batchId;
    result.batchPublicId = `batch-${batchId}`;
    return result;
  }

  if (active.startsWith("quality_")
      && cmd === "exam_fixed_intake_confirm_grouping_quality") {
    window.__fixedCalls.push({ cmd, args });
    const rejectedPageIds = [...args.rejectedPageIds];
    if (active.startsWith("quality_evidence_")) {
      window.__qualityConfirmed = true;
      window.__qualityLifecycleConfirmed = true;
      window.__rejectedPageIds = rejectedPageIds;
      return qualityConfirmation(rejectedPageIds);
    }
    return new Promise((resolve, reject) => {
      window.__qualityLifecycleResolvers.push({
        resolve: () => {
          window.__qualityConfirmed = true;
          window.__qualityLifecycleConfirmed = true;
          window.__rejectedPageIds = rejectedPageIds;
          resolve(qualityConfirmation(rejectedPageIds));
        },
        reject: () => reject(new Error("模拟旧质量确认失败")),
      });
    });
  }

  if (active.startsWith("quality_evidence_")
      && window.__qualityLifecycleConfirmed
      && cmd === "exam_fixed_intake_grouping_evidence") {
    window.__fixedCalls.push({ cmd, args });
    const evidence = groupingEvidence();
    return new Promise((resolve, reject) => {
      window.__qualityEvidenceResolvers.push({
        resolve: () => resolve(evidence),
        reject: () => reject(new Error("模拟旧质量证据读取失败")),
      });
    });
  }

  if ((active === "retake_double" || active === "retake_dialog_stale")
      && cmd === "plugin:dialog|open"
      && window.__dialogCount >= 1) {
    window.__fixedCalls.push({ cmd, args });
    window.__dialogCount += 1;
    return new Promise((resolve) => {
      window.__retakeDialogResolvers.push(() => resolve("/tmp/RETAKE_0002.jpg"));
    });
  }

  if (active.startsWith("retake_") && cmd === "exam_fixed_intake_replace_rejected_page") {
    window.__fixedCalls.push({ cmd, args });
    if (active.startsWith("retake_evidence_")) {
      window.__replaced = true;
      window.__retakeLifecycleSucceeded = true;
      return retakeReplacement();
    }
    return new Promise((resolve, reject) => {
      window.__retakeLifecycleResolvers.push({
        resolve: () => {
          window.__replaced = true;
          window.__retakeLifecycleSucceeded = true;
          resolve(retakeReplacement());
        },
        reject: () => reject(new Error("模拟旧重拍替换失败")),
      });
    });
  }

  if (active.startsWith("retake_evidence_")
      && window.__retakeLifecycleSucceeded
      && cmd === "exam_fixed_intake_grouping_evidence") {
    window.__fixedCalls.push({ cmd, args });
    const evidence = groupingEvidence();
    return new Promise((resolve, reject) => {
      window.__retakeEvidenceResolvers.push({
        resolve: () => resolve(evidence),
        reject: () => reject(new Error("模拟旧重拍证据读取失败")),
      });
    });
  }

  return qualityRetakeBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{QUALITY_RETAKE_LIFECYCLE_OVERRIDE}")


def prepare_without_answer(page: Page) -> None:
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()


def switch_assessment_and_prepare(page: Page) -> None:
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()


def reach_quality_review(page: Page) -> None:
    prepare_without_answer(page)
    page.get_by_role("button", name="普通试卷").click()
    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()
    page.get_by_role("button", name="确认这 3 份学生顺序").click()
    expect(page.get_by_text("看一眼照片是否清楚", exact=True)).to_be_visible()


def reach_single_retake(page: Page) -> None:
    reach_quality_review(page)
    page.locator(".intake-page-thumb").nth(1).click()
    page.get_by_role("button", name="确认清楚页面，扣住 1 页重拍").click()
    expect(page.get_by_role("button", name="2号 李同学 · 第2页重拍")).to_be_visible()


def test_quality_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=quality_double")
    reach_quality_review(page)
    button = page.get_by_role("button", name="照片都清楚，确认并建立归属")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__qualityLifecycleResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_fixed_intake_confirm_grouping_quality")) == 1
    page.evaluate("window.__qualityLifecycleResolvers[0].resolve()")
    expect(page.get_by_text("页面质量与正式归属已确认", exact=True)).to_be_visible()
    assert_no_authority_writes(page)
    page.close()


def test_old_quality_success_stops_before_evidence(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=quality_stale_success")
    reach_quality_review(page)
    evidence_count = len(calls(page, "exam_fixed_intake_grouping_evidence"))
    page.get_by_role("button", name="照片都清楚，确认并建立归属").click()
    page.wait_for_function("window.__qualityLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__qualityLifecycleResolvers[0].resolve()")
    page.wait_for_timeout(150)

    assert len(calls(page, "exam_fixed_intake_grouping_evidence")) == evidence_count
    assert page.get_by_text("页面质量与正式归属已确认", exact=True).count() == 0
    assert calls(page, "exam_ordinary_paper_analyze_page") == []
    assert_no_authority_writes(page)
    page.close()


def test_old_quality_failure_is_silent(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=quality_stale_failure")
    reach_quality_review(page)
    page.get_by_role("button", name="照片都清楚，确认并建立归属").click()
    page.wait_for_function("window.__qualityLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__qualityLifecycleResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧质量确认失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_old_quality_evidence_success_cannot_continue(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=quality_evidence_stale_success")
    reach_quality_review(page)
    page.get_by_role("button", name="照片都清楚，确认并建立归属").click()
    page.wait_for_function("window.__qualityEvidenceResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__qualityEvidenceResolvers[0].resolve()")
    page.wait_for_timeout(150)

    assert calls(page, "exam_ordinary_paper_analyze_page") == []
    assert page.get_by_text("只确认一次，这批是什么？", exact=True).count() == 1
    assert_no_authority_writes(page)
    page.close()


def test_old_quality_evidence_failure_is_silent(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=quality_evidence_stale_failure")
    reach_quality_review(page)
    page.get_by_role("button", name="照片都清楚，确认并建立归属").click()
    page.wait_for_function("window.__qualityEvidenceResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__qualityEvidenceResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧质量证据读取失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_retake_same_turn_opens_one_dialog_and_calls_provider_once(
    browser: Browser,
    base_url: str,
) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=retake_double")
    reach_single_retake(page)
    button = page.get_by_role("button", name="2号 李同学 · 第2页重拍")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__retakeDialogResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "plugin:dialog|open")) == 2
    page.evaluate("window.__retakeDialogResolvers[0]()")
    page.wait_for_function("window.__retakeLifecycleResolvers.length >= 1")
    assert len(calls(page, "exam_fixed_intake_replace_rejected_page")) == 1
    page.evaluate("window.__retakeLifecycleResolvers[0].resolve()")
    expect(page.locator(".intake-quality-complete")).to_contain_text(
        "已进入后续识别：3 名；需重拍：0 名。"
    )
    assert_no_authority_writes(page)
    page.close()


def test_stale_retake_dialog_cannot_call_provider(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=retake_dialog_stale")
    reach_single_retake(page)
    page.get_by_role("button", name="2号 李同学 · 第2页重拍").click()
    page.wait_for_function("window.__retakeDialogResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__retakeDialogResolvers[0]()")
    page.wait_for_timeout(150)

    assert calls(page, "exam_fixed_intake_replace_rejected_page") == []
    assert_no_authority_writes(page)
    page.close()


def test_old_retake_success_stops_before_evidence(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=retake_stale_success")
    reach_single_retake(page)
    evidence_count = len(calls(page, "exam_fixed_intake_grouping_evidence"))
    ordinary_count = len(calls(page, "exam_ordinary_paper_analyze_page"))
    page.get_by_role("button", name="2号 李同学 · 第2页重拍").click()
    page.wait_for_function("window.__retakeLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__retakeLifecycleResolvers[0].resolve()")
    page.wait_for_timeout(150)

    assert len(calls(page, "exam_fixed_intake_grouping_evidence")) == evidence_count
    assert len(calls(page, "exam_ordinary_paper_analyze_page")) == ordinary_count
    assert_no_authority_writes(page)
    page.close()


def test_old_retake_failure_is_silent(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=retake_stale_failure")
    reach_single_retake(page)
    page.get_by_role("button", name="2号 李同学 · 第2页重拍").click()
    page.wait_for_function("window.__retakeLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__retakeLifecycleResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧重拍替换失败").count() == 0
    assert_no_authority_writes(page)
    page.close()


def test_old_retake_evidence_success_cannot_continue(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=retake_evidence_stale_success")
    reach_single_retake(page)
    ordinary_count = len(calls(page, "exam_ordinary_paper_analyze_page"))
    page.get_by_role("button", name="2号 李同学 · 第2页重拍").click()
    page.wait_for_function("window.__retakeEvidenceResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__retakeEvidenceResolvers[0].resolve()")
    page.wait_for_timeout(150)

    assert len(calls(page, "exam_ordinary_paper_analyze_page")) == ordinary_count
    assert page.get_by_text("只确认一次，这批是什么？", exact=True).count() == 1
    assert_no_authority_writes(page)
    page.close()


def test_old_retake_evidence_failure_is_silent(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&qualityRetake=retake_evidence_stale_failure")
    reach_single_retake(page)
    page.get_by_role("button", name="2号 李同学 · 第2页重拍").click()
    page.wait_for_function("window.__retakeEvidenceResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__retakeEvidenceResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧重拍证据读取失败").count() == 0
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


def test_quality_retake_lifecycle(base_url: str) -> None:
    cases = [
        ("quality same-turn reentry", test_quality_same_turn_calls_provider_once),
        ("stale quality success", test_old_quality_success_stops_before_evidence),
        ("stale quality failure", test_old_quality_failure_is_silent),
        ("stale quality evidence success", test_old_quality_evidence_success_cannot_continue),
        ("stale quality evidence failure", test_old_quality_evidence_failure_is_silent),
        ("retake same-turn reentry", test_retake_same_turn_opens_one_dialog_and_calls_provider_once),
        ("stale retake dialog", test_stale_retake_dialog_cannot_call_provider),
        ("stale retake success", test_old_retake_success_stops_before_evidence),
        ("stale retake failure", test_old_retake_failure_is_silent),
        ("stale retake evidence success", test_old_retake_evidence_success_cannot_continue),
        ("stale retake evidence failure", test_old_retake_evidence_failure_is_silent),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(f"R3-H1-S7 quality/retake lifecycle still RED: {failed_names}")


if __name__ == "__main__":
    test_quality_retake_lifecycle("http://127.0.0.1:4173")
    print("fixed intake quality/retake lifecycle UI: PASS")
