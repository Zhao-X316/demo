"""R3-H1-S6 材料类型与学生归组确认生命周期测试。"""

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


MATERIAL_GROUPING_LIFECYCLE_OVERRIDE = r"""
const materialGroupingBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
const materialGroupingScenario = () => params().get("materialGrouping") || "";
window.__materialLifecycleResolvers = [];
window.__groupingLifecycleResolvers = [];

const materialConfirmation = (args) => ({
  materialType: args.materialType,
  materialTypeDecision: "teacher_confirmed",
  materialTypeConfidence: 1,
  groupingRoute: "preview_ready",
  studentGroupCount: 3,
  groupingIssueCodes: [],
  nextAction: "确认学生顺序",
});

const groupingConfirmation = (args) => ({
  groupingRoute: "preview_ready",
  studentGroupCount: 3,
  groupingIssueCodes: [],
  groupingConfirmed: true,
  groupingFirstStudentNo: args.firstStudentNo,
  groupingLastStudentNo: "3",
  nextAction: "确认照片清晰度",
});

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const active = materialGroupingScenario();

  if (active.includes("stale") && cmd === "exam_fixed_intake_prepare") {
    window.__fixedCalls.push({ cmd, args });
    window.__prepareAttempts += 1;
    const result = preparedResult(args.request);
    const batchId = window.__prepareAttempts === 1 ? 101 : 102;
    result.batchId = batchId;
    result.batchPublicId = `batch-${batchId}`;
    return result;
  }

  if (active.startsWith("material_") && cmd === "exam_fixed_intake_confirm_material_type") {
    window.__fixedCalls.push({ cmd, args });
    return new Promise((resolve, reject) => {
      window.__materialLifecycleResolvers.push({
        resolve: () => resolve(materialConfirmation(args)),
        reject: () => reject(new Error("模拟旧材料确认失败")),
      });
    });
  }

  if (active.startsWith("grouping_") && cmd === "exam_fixed_intake_confirm_grouping") {
    window.__fixedCalls.push({ cmd, args });
    return new Promise((resolve, reject) => {
      window.__groupingLifecycleResolvers.push({
        resolve: () => resolve(groupingConfirmation(args)),
        reject: () => reject(new Error("模拟旧归组确认失败")),
      });
    });
  }

  return materialGroupingBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{MATERIAL_GROUPING_LIFECYCLE_OVERRIDE}")


def prepare_without_answer(page: Page) -> None:
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()


def switch_assessment_and_prepare(page: Page) -> None:
    page.get_by_label("批改哪份作业").select_option("13")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("只确认一次，这批是什么？", exact=True)).to_be_visible()


def reach_grouping(page: Page) -> None:
    prepare_without_answer(page)
    page.get_by_role("button", name="普通试卷").click()
    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()


def test_material_choices_share_one_owner(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&materialGrouping=material_cross")
    prepare_without_answer(page)
    page.evaluate("""
      const buttons = [...document.querySelectorAll("button")];
      buttons.find((node) => node.textContent === "普通试卷").click();
      buttons.find((node) => node.textContent === "答题卡").click();
    """)
    page.wait_for_function("window.__materialLifecycleResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_fixed_intake_confirm_material_type")) == 1
    page.evaluate("window.__materialLifecycleResolvers[0].resolve()")
    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()
    assert_no_authority_writes(page)
    page.close()


def test_old_material_success_cannot_update_new_batch(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&materialGrouping=material_stale_success")
    prepare_without_answer(page)
    page.get_by_role("button", name="普通试卷").click()
    page.wait_for_function("window.__materialLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__materialLifecycleResolvers[0].resolve()")
    page.wait_for_timeout(150)

    assert page.get_by_text("只确认一次，这批是什么？", exact=True).count() == 1
    assert page.get_by_text("确认照片从哪位学生开始", exact=True).count() == 0
    assert page.get_by_label("批改哪份作业").input_value() == "13"
    assert_no_authority_writes(page)
    page.close()


def test_old_material_failure_is_silent(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&materialGrouping=material_stale_failure")
    prepare_without_answer(page)
    page.get_by_role("button", name="普通试卷").click()
    page.wait_for_function("window.__materialLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__materialLifecycleResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧材料确认失败").count() == 0
    assert page.get_by_text("只确认一次，这批是什么？", exact=True).count() == 1
    assert_no_authority_writes(page)
    page.close()


def test_grouping_same_turn_calls_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&materialGrouping=grouping_double")
    reach_grouping(page)
    button = page.get_by_role("button", name="确认这 3 份学生顺序")
    button.evaluate("node => { node.click(); node.click(); }")
    page.wait_for_function("window.__groupingLifecycleResolvers.length >= 1")
    page.wait_for_timeout(100)

    assert len(calls(page, "exam_fixed_intake_confirm_grouping")) == 1
    page.evaluate("window.__groupingLifecycleResolvers[0].resolve()")
    expect(page.get_by_text("照片与学生顺序已确认", exact=True)).to_be_visible()
    assert_no_authority_writes(page)
    page.close()


def test_old_grouping_success_cannot_confirm_new_batch(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1150})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&materialGrouping=grouping_stale_success")
    reach_grouping(page)
    page.get_by_role("button", name="确认这 3 份学生顺序").click()
    page.wait_for_function("window.__groupingLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__groupingLifecycleResolvers[0].resolve()")
    page.wait_for_timeout(100)
    page.get_by_role("button", name="普通试卷").click()
    expect(page.get_by_text("确认照片从哪位学生开始", exact=True)).to_be_visible()

    assert page.get_by_text("照片与学生顺序已确认", exact=True).count() == 0
    assert page.get_by_label("批改哪份作业").input_value() == "13"
    assert_no_authority_writes(page)
    page.close()


def test_old_grouping_failure_is_silent(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=options_matrix&materialGrouping=grouping_stale_failure")
    reach_grouping(page)
    page.get_by_role("button", name="确认这 3 份学生顺序").click()
    page.wait_for_function("window.__groupingLifecycleResolvers.length === 1")
    switch_assessment_and_prepare(page)
    page.evaluate("window.__groupingLifecycleResolvers[0].reject()")
    page.wait_for_timeout(150)

    assert page.locator(".error").filter(has_text="模拟旧归组确认失败").count() == 0
    assert page.get_by_text("只确认一次，这批是什么？", exact=True).count() == 1
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


def test_material_grouping_lifecycle(base_url: str) -> None:
    cases = [
        ("material choices shared owner", test_material_choices_share_one_owner),
        ("stale material success", test_old_material_success_cannot_update_new_batch),
        ("stale material failure", test_old_material_failure_is_silent),
        ("grouping same-turn reentry", test_grouping_same_turn_calls_provider_once),
        ("stale grouping success", test_old_grouping_success_cannot_confirm_new_batch),
        ("stale grouping failure", test_old_grouping_failure_is_silent),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(f"R3-H1-S6 material/grouping lifecycle still RED: {failed_names}")


if __name__ == "__main__":
    test_material_grouping_lifecycle("http://127.0.0.1:4173")
    print("fixed intake material/grouping lifecycle UI: PASS")
