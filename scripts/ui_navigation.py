"""Shared navigation for component regression tests in the simplified app shell."""

def enter_exam(page):
    page.get_by_role("navigation", name="主要导航").get_by_role("button", name="工作台", exact=True).click()
    if not page.get_by_role("heading", name="题目批改", exact=True).is_visible():
        back = page.get_by_role("button", name="← 返回工作台", exact=True)
        if back.is_visible():
            back.click()
        page.get_by_role("button", name="新建批改", exact=True).click()
        page.get_by_role("button", name="作业", exact=True).click()
    tools = page.locator(".task-tools:visible")
    if tools.get_attribute("open") is None:
        tools.get_by_text("更多批改工具", exact=True).click()


def enter_materials(page):
    page.get_by_role("navigation", name="主要导航").get_by_role("button", name="资料", exact=True).click()
    tools=page.get_by_text("整理与组卷",exact=True)
    if tools.locator("..").get_attribute("open") is None:
        tools.click()


def enter_learning(page):
    page.get_by_role("navigation", name="主要导航").get_by_role("button", name="学生", exact=True).click()
    page.get_by_role("button", name="错题与掌握", exact=True).click()


def enter_dashboard(page):
    page.get_by_role("navigation", name="主要导航").get_by_role("button", name="学生", exact=True).click()
    page.get_by_role("button", name="班级学习情况", exact=True).click()
    rules=page.get_by_text("本页统计口径",exact=True)
    if rules.is_visible() and rules.locator("..").get_attribute("open") is None:
        rules.click()


def expand_new_intake(page):
    tool=page.get_by_text("另建一份批改", exact=True)
    if tool.is_visible() and tool.locator("..").get_attribute("open") is None:
        tool.click()
