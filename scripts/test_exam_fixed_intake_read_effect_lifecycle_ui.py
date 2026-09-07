"""R3-H1-S4 固定上传 read-effect 与直接续处理生命周期测试。

只复用共享固定上传 mock，延迟模板状态读取或同轮重入失败页重试；
不调用真实 provider、正式数据库、老师终审、评分或发布命令。
"""

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


READ_EFFECT_OVERRIDE_SCRIPT = r"""
const readEffectBaseInvoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
window.__heldAnswerSheetStatus = false;
window.__heldDictationStatus = false;
window.__dictationRetryInjected = false;

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  const activeScenario = scenario();

  if (activeScenario === "read_effect_answer_sheet_batch_reset"
      && cmd === "exam_answer_sheet_template_status"
      && args.referencePageId === 301
      && !window.__heldAnswerSheetStatus) {
    window.__heldAnswerSheetStatus = true;
    const status = readEffectBaseInvoke(cmd, args);
    return new Promise((resolve) => {
      window.__resolveHeldAnswerSheetStatus = async () => resolve(await status);
    });
  }

  if (activeScenario === "read_effect_dictation_batch_reset"
      && cmd === "exam_dictation_template_status"
      && args.referencePageId === 301
      && !window.__heldDictationStatus) {
    window.__heldDictationStatus = true;
    const status = readEffectBaseInvoke(cmd, args);
    return new Promise((resolve) => {
      window.__resolveHeldDictationStatus = async () => resolve(await status);
    });
  }

  if (activeScenario === "dictation_retry_dedupe"
      && cmd === "exam_dictation_process_page"
      && !window.__dictationRetryInjected) {
    window.__dictationRetryInjected = true;
    window.__fixedCalls.push({ cmd, args });
    window.__dictationProcessAttempts += 1;
    throw new Error("模拟首轮默写处理失败");
  }

  return readEffectBaseInvoke(cmd, args);
};
"""


def install_mock(page: Page) -> None:
    page.add_init_script(f"{MOCK_SCRIPT}\n{READ_EFFECT_OVERRIDE_SCRIPT}")


def test_stale_answer_sheet_status_cannot_start_old_page(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(
        page,
        f"{base_url}?scenario=read_effect_answer_sheet_batch_reset&material=answer_sheet",
    )
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    page.wait_for_function("window.__heldAnswerSheetStatus === true")

    select_student_papers(page, "NEW_0001.jpg", "NEW_0004.jpg")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("已处理 1/1 页", exact=True)).to_be_visible()

    page.evaluate("window.__resolveHeldAnswerSheetStatus()")
    page.wait_for_timeout(150)

    status_pages = [
        item["args"]["referencePageId"]
        for item in calls(page, "exam_answer_sheet_template_status")
    ]
    processed_pages = [item["args"]["pageId"] for item in calls(page, "exam_answer_sheet_process_page")]
    assert status_pages == [301, 401], status_pages
    assert processed_pages == [401], processed_pages
    assert_no_authority_writes(page)
    page.close()


def test_stale_dictation_status_cannot_start_old_page(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(
        page,
        f"{base_url}?scenario=read_effect_dictation_batch_reset&material=dictation",
    )
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    page.wait_for_function("window.__heldDictationStatus === true")

    select_student_papers(page, "NEW_0001.jpg", "NEW_0004.jpg")
    page.get_by_role("button", name="上传并开始整理").click()
    expect(page.get_by_text("精确命中 1", exact=True)).to_be_visible()

    page.evaluate("window.__resolveHeldDictationStatus()")
    page.wait_for_timeout(150)

    status_pages = [
        item["args"]["referencePageId"]
        for item in calls(page, "exam_dictation_template_status")
    ]
    processed_pages = [item["args"]["pageId"] for item in calls(page, "exam_dictation_process_page")]
    assert status_pages == [301, 401], status_pages
    assert processed_pages == [401], processed_pages
    assert_no_authority_writes(page)
    page.close()


def test_answer_sheet_retry_reentry_calls_page_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=answer_sheet_batch_reset&material=answer_sheet")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    retry = page.get_by_role("button", name="重试 1 张失败卡")
    expect(retry).to_be_visible()

    retry.evaluate("node => { node.click(); node.click(); }")
    expect(page.get_by_text("已处理 1/1 页", exact=True)).to_be_visible()
    page.wait_for_timeout(100)

    processed_pages = [item["args"]["pageId"] for item in calls(page, "exam_answer_sheet_process_page")]
    assert processed_pages == [301, 301], processed_pages
    assert_no_authority_writes(page)
    page.close()


def test_dictation_retry_reentry_calls_page_provider_once(browser: Browser, base_url: str) -> None:
    page = browser.new_page(viewport={"width": 1440, "height": 1100})
    install_mock(page)
    open_exam(page, f"{base_url}?scenario=dictation_retry_dedupe&material=dictation")
    select_student_papers(page)
    page.get_by_role("button", name="上传并开始整理").click()
    retry = page.get_by_role("button", name="重试 1 张失败默写")
    expect(retry).to_be_visible()

    retry.evaluate("node => { node.click(); node.click(); }")
    expect(page.get_by_text("精确命中 1", exact=True)).to_be_visible()
    page.wait_for_timeout(100)

    processed_pages = [item["args"]["pageId"] for item in calls(page, "exam_dictation_process_page")]
    assert processed_pages == [301, 301], processed_pages
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
    except Exception as error:  # noqa: BLE001 - aggregate every independent case
        print(f"FAIL {name}: {error}")
        return name, str(error)


def test_fixed_intake_read_effect_lifecycle(base_url: str) -> None:
    cases = [
        ("stale answer-sheet status", test_stale_answer_sheet_status_cannot_start_old_page),
        ("stale dictation status", test_stale_dictation_status_cannot_start_old_page),
        ("answer-sheet retry reentry", test_answer_sheet_retry_reentry_calls_page_provider_once),
        ("dictation retry reentry", test_dictation_retry_reentry_calls_page_provider_once),
    ]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        results = [run_case(browser, base_url, name, case) for name, case in cases]
        browser.close()

    failures = [(name, error) for name, error in results if error is not None]
    if failures:
        failed_names = ", ".join(name for name, _ in failures)
        raise AssertionError(f"R3-H1-S4 read-effect lifecycle failed: {failed_names}")


if __name__ == "__main__":
    test_fixed_intake_read_effect_lifecycle("http://127.0.0.1:4173")
    print("fixed intake read-effect lifecycle UI: PASS")
