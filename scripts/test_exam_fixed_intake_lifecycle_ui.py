from ui_navigation import expand_new_intake
"""R3-H1 固定上传生命周期风险红测试。

该脚本只扩展既有共享 mock，不修改生产请求，也不触发老师终审、计分或发布。
H1 生产加固完成前，至少“旧页周期回写”和“旧 prepare 回写”应稳定 RED。
"""

from collections.abc import Callable
from typing import Optional, Tuple

from playwright.sync_api import Browser, Page, sync_playwright

from test_exam_fixed_intake_shared_shell_ui import (
    MOCK_SCRIPT,
    assert_no_authority_writes,
    calls,
    open_exam,
    select_student_papers,
)


LIFECYCLE_OVERRIDE_SCRIPT = r"""
const lifecycleBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const lifecycleScenario = () => params().get("lifecycle") || scenario();
window.__cycleAttempts = 0;
window.__firstCycleResolved = false;
window.__prepareResolvers = [];

const cycleResult = (pages) => ({
  expectedPagesPerAttempt: pages,
  confidence: 0.96,
  source: "visual_repeating_layout_v1",
  issueCodes: [],
  needsTeacherInput: false,
});

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const activeScenario = lifecycleScenario();

  if (activeScenario === "lifecycle_cycle_stale" && cmd === "plugin:dialog|open") {
    window.__fixedCalls.push({ cmd, args });
    window.__dialogCount += 1;
    const prefix = window.__dialogCount === 1 ? "OLD" : "NEW";
    return Array.from({ length: 6 }, (_, index) => `/tmp/${prefix}_000${index + 1}.jpg`);
  }

  if (activeScenario === "lifecycle_cycle_stale" && cmd === "exam_fixed_intake_infer_page_cycle") {
    window.__fixedCalls.push({ cmd, args });
    window.__cycleAttempts += 1;
    if (window.__cycleAttempts === 1) {
      return new Promise((resolve) => {
        window.__resolveFirstCycle = () => {
          window.__firstCycleResolved = true;
          resolve(cycleResult(2));
        };
      });
    }
    return cycleResult(3);
  }

  if ((activeScenario === "lifecycle_prepare_stale"
      || activeScenario === "lifecycle_prepare_double")
      && cmd === "exam_fixed_intake_prepare") {
    window.__fixedCalls.push({ cmd, args });
    window.__prepareAttempts += 1;
    return new Promise((resolve) => {
      window.__prepareResolvers.push(() => resolve(preparedResult(args.request)));
    });
  }

  return lifecycleBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{LIFECYCLE_OVERRIDE_SCRIPT}")


def test_old_page_cycle_cannot_overwrite_new_selection(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=lifecycle_cycle_stale")

    page.get_by_role("button", name="选择试卷").click()
    page.get_by_text("OLD_0001.jpg", exact=True).wait_for()
    page.wait_for_function("window.__cycleAttempts === 1")

    page.get_by_role("button", name="选择试卷").click()
    page.get_by_text("NEW_0001.jpg", exact=True).wait_for()
    page.get_by_text("检测到版式每 3 页重复 · 可修改", exact=True).wait_for()

    page.evaluate("window.__resolveFirstCycle()")
    page.wait_for_function("window.__firstCycleResolved === true")
    page.wait_for_timeout(150)

    new_count = page.get_by_text("检测到版式每 3 页重复 · 可修改", exact=True).count()
    stale_count = page.get_by_text("检测到版式每 2 页重复 · 可修改", exact=True).count()
    assert new_count == 1 and stale_count == 0, (
        f"expected 3-page suggestion only; observed new={new_count}, stale={stale_count}"
    )
    assert_no_authority_writes(page)
    page.close()


def test_old_prepare_cannot_repopulate_switched_assessment(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&lifecycle=lifecycle_prepare_stale")
    select_student_papers(page)

    page.get_by_role("button", name="上传并开始整理").click()
    page.wait_for_function("window.__prepareAttempts === 1")
    expand_new_intake(page)
    page.get_by_label("批改哪份作业").select_option("13")
    assert page.get_by_label("批改哪份作业").input_value() == "13"

    page.evaluate("window.__prepareResolvers[0]()")
    page.wait_for_timeout(150)

    stale_result_count = page.get_by_text("已归档 6 份学生卷，共 6 页", exact=True).count()
    assert stale_result_count == 0, (
        f"expected switched assessment to ignore old prepare; observed old result={stale_result_count}"
    )
    assert page.get_by_label("批改哪份作业").input_value() == "13"
    assert_no_authority_writes(page)
    page.close()


def test_same_turn_prepare_reentry_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1000})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=main&lifecycle=lifecycle_prepare_double")
    select_student_papers(page)

    button = page.get_by_role("button", name="上传并开始整理")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__prepareAttempts >= 1")
    page.wait_for_timeout(100)

    prepare_call_count = len(calls(page, "exam_fixed_intake_prepare"))
    assert prepare_call_count == 1, (
        f"expected one provider call for same-turn reentry; observed {prepare_call_count}"
    )
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
    except Exception as error:  # noqa: BLE001 - aggregate every independent red case
        print(f"RED  {name}: {error}")
        return name, str(error)


def test_fixed_intake_lifecycle(base_url: str) -> None:
    cases = [
        ("stale page-cycle completion", test_old_page_cycle_cannot_overwrite_new_selection),
        ("stale prepare completion", test_old_prepare_cannot_repopulate_switched_assessment),
        ("same-turn prepare reentry", test_same_turn_prepare_reentry_calls_provider_once),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(f"R3-H1 lifecycle target still RED: {failed_names}")


if __name__ == "__main__":
    test_fixed_intake_lifecycle("http://127.0.0.1:4173")
    print("fixed intake lifecycle UI: PASS")
