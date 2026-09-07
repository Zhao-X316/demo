"""固定 SubjectiveLinkPanel 的加载、编辑、确认和保存失败行为。"""

from playwright.sync_api import Page, expect, sync_playwright

from test_m25_rubric_update_ui import MOCK_SCRIPT as M25_BASE_MOCK_SCRIPT


LINK_MOCK_SCRIPT = M25_BASE_MOCK_SCRIPT + r"""
window.__linkCalls = [];
window.__linkSavedSources = null;
const __baseInvokeForSubjectiveLinks = window.__TAURI_INTERNALS__.invoke;

function linkSourceMeta(sourcePublicId) {
  if (sourcePublicId === "slot-year") {
    return {
      source_type: "answer_slot",
      source_public_id: "slot-year",
      stable_id: "year",
      order_index: 0,
      label: "年份",
      max_score: 1,
    };
  }
  return {
    source_type: "rubric_point",
    source_public_id: "rubric-institution",
    stable_id: "institution",
    order_index: 1,
    label: "制度局限",
    max_score: 4,
  };
}

function knowledgeView(link) {
  const titles = { 101: "洋务运动制度局限", 102: "洋务企业管理", 103: "列强限制" };
  return {
    knowledge_node_id: link.knowledge_node_id,
    knowledge_node_public_id: `knowledge-${link.knowledge_node_id}`,
    knowledge_title: titles[link.knowledge_node_id],
    relation_type: link.relation_type,
  };
}

function abilityView(link) {
  const titles = { 201: "事实识记", 202: "因果分析", 203: "历史解释" };
  return {
    ability_dimension_id: link.ability_dimension_id,
    ability_dimension_public_id: `ability-${link.ability_dimension_id}`,
    ability_title: titles[link.ability_dimension_id],
    evidence_strength: link.evidence_strength,
    response_mode: link.response_mode,
  };
}

function subjectiveLinkEditor() {
  const initialSources = [
    {
      ...linkSourceMeta("slot-year"),
      knowledge_links: [],
      ability_links: [],
    },
    {
      ...linkSourceMeta("rubric-institution"),
      knowledge_links: [{
        knowledge_node_id: 101,
        knowledge_node_public_id: "knowledge-101",
        knowledge_title: "洋务运动制度局限",
        relation_type: "rubric_basis",
      }],
      ability_links: [{
        ability_dimension_id: 201,
        ability_dimension_public_id: "ability-201",
        ability_title: "事实识记",
        evidence_strength: 0.6,
        response_mode: "structured_response",
      }],
    },
  ];
  const sources = window.__linkSavedSources
    ? window.__linkSavedSources.map((source) => ({
        ...linkSourceMeta(source.source_public_id),
        knowledge_links: source.knowledge_links.map(knowledgeView),
        ability_links: source.ability_links.map(abilityView),
      }))
    : initialSources;
  return {
    source_assessment_item_id: 2,
    assessment_id: 1,
    base_assessment_version_id: 1,
    base_assessment_revision: 1,
    base_assessment_item_id: 2,
    question_version_id: 2,
    question_type: "short_answer",
    question_no: "2",
    question_stem: "概括洋务运动失败的原因",
    link_set_id: 2,
    link_set_revision: 1,
    sources,
    knowledge_options: [
      { id: 101, public_id: "knowledge-101", code: "HIS-01", title: "洋务运动制度局限" },
      { id: 102, public_id: "knowledge-102", code: "HIS-02", title: "洋务企业管理" },
      { id: 103, public_id: "knowledge-103", code: null, title: "列强限制" },
    ],
    ability_options: [
      { id: 201, public_id: "ability-201", code: "recall", title: "事实识记" },
      { id: 202, public_id: "ability-202", code: "cause", title: "因果分析" },
      { id: 203, public_id: "ability-203", code: "explain", title: "历史解释" },
    ],
  };
}

window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
  if (cmd === "exam_subjective_link_editor") {
    window.__linkCalls.push({ cmd, args: structuredClone(args) });
    if (new URLSearchParams(window.location.search).has("linkLoadFail")) {
      throw new Error("LINK_LOAD_FAILED");
    }
    return subjectiveLinkEditor();
  }
  if (cmd === "exam_subjective_link_save") {
    window.__linkCalls.push({ cmd, args: structuredClone(args) });
    if (new URLSearchParams(window.location.search).has("linkSaveFail")) {
      throw new Error("LINK_SAVE_FAILED");
    }
    window.__linkSavedSources = structuredClone(args.sources);
    return {
      outcome: "created_new_version",
      edit_id: 30,
      adopted_assessment_version_id: 2,
      adopted_assessment_revision: 2,
      adopted_link_set_id: 3,
      adopted_link_set_revision: 1,
      knowledge_link_count: 2,
      ability_link_count: 3,
      current_attempts_unchanged: true,
      current_publications_unchanged: true,
      historical_evidence_unchanged: true,
    };
  }
  return __baseInvokeForSubjectiveLinks(cmd, args);
};
"""


def open_subjective_links(page: Page, url: str) -> None:
    page.add_init_script(LINK_MOCK_SCRIPT)
    page.goto(url)
    page.wait_for_load_state("networkidle")
    page.locator(".mod-row").filter(has_text="改作业").click()
    page.get_by_role("button", name="题目批改").click()
    page.get_by_role("button", name="答题卡主观题").click()


def expand_link_panel(page: Page):
    panel = page.locator("details.subjective-link-panel")
    expect(panel).to_have_count(1)
    if panel.get_attribute("open") is None:
        panel.locator("summary").click()
    expect(panel.get_by_text("用于未来作业和学习图谱", exact=True)).to_be_visible()
    return panel


def assert_initial_editor_call(page: Page) -> None:
    calls = page.evaluate(
        "window.__linkCalls.filter((item) => item.cmd === 'exam_subjective_link_editor')"
    )
    assert calls == [{"cmd": "exam_subjective_link_editor", "args": {"assessmentItemId": 2}}]


def edit_links(panel) -> None:
    sources = panel.locator("article.subjective-link-source")
    expect(sources).to_have_count(2)
    answer_source = sources.nth(0)
    rubric_source = sources.nth(1)
    expect(answer_source.get_by_text("年份", exact=True)).to_be_visible()
    expect(rubric_source.get_by_text("制度局限", exact=True)).to_be_visible()

    answer_knowledge = answer_source.locator(".subjective-link-list").nth(0)
    answer_ability = answer_source.locator(".subjective-link-list").nth(1)
    rubric_knowledge = rubric_source.locator(".subjective-link-list").nth(0)
    rubric_ability = rubric_source.locator(".subjective-link-list").nth(1)

    expect(answer_knowledge.locator(".subjective-link-row")).to_have_count(0)
    answer_knowledge.get_by_role("button", name="＋ 知识点").click()
    answer_knowledge_row = answer_knowledge.locator(".subjective-link-row")
    expect(answer_knowledge_row.locator("select").nth(0)).to_have_value("101")
    expect(answer_knowledge_row.locator("select").nth(1)).to_have_value("answer_basis")
    answer_knowledge_row.get_by_role("button", name="移除").click()
    expect(answer_knowledge.locator(".subjective-link-row")).to_have_count(0)

    answer_ability.get_by_role("button", name="＋ 能力维度").click()
    answer_ability_row = answer_ability.locator(".subjective-link-row")
    expect(answer_ability_row.locator("select").nth(0)).to_have_value("201")
    expect(answer_ability_row.locator("select").nth(1)).to_have_value("recall")
    expect(answer_ability_row.locator("input")).to_have_value("0.5")

    expect(rubric_knowledge.locator(".subjective-link-row")).to_have_count(1)
    rubric_knowledge.get_by_role("button", name="＋ 知识点").click()
    expect(rubric_knowledge.locator(".subjective-link-row")).to_have_count(2)
    new_knowledge = rubric_knowledge.locator(".subjective-link-row").nth(1)
    expect(new_knowledge.locator("select").nth(0)).to_have_value("102")
    expect(new_knowledge.locator("select").nth(1)).to_have_value("rubric_basis")

    expect(rubric_ability.locator(".subjective-link-row")).to_have_count(1)
    rubric_ability.get_by_role("button", name="＋ 能力维度").click()
    expect(rubric_ability.locator(".subjective-link-row")).to_have_count(2)
    new_ability = rubric_ability.locator(".subjective-link-row").nth(1)
    expect(new_ability.locator("select").nth(0)).to_have_value("202")
    expect(new_ability.locator("select").nth(1)).to_have_value("structured_response")
    expect(new_ability.locator("input")).to_have_value("0.7")
    new_ability.locator("select").nth(1).select_option("source_analysis")
    new_ability.locator("input").fill("0.9")


EXPECTED_SOURCES = [
    {
        "source_type": "answer_slot",
        "source_public_id": "slot-year",
        "knowledge_links": [],
        "ability_links": [
            {
                "ability_dimension_id": 201,
                "evidence_strength": 0.5,
                "response_mode": "recall",
            }
        ],
    },
    {
        "source_type": "rubric_point",
        "source_public_id": "rubric-institution",
        "knowledge_links": [
            {"knowledge_node_id": 101, "relation_type": "rubric_basis"},
            {"knowledge_node_id": 102, "relation_type": "rubric_basis"},
        ],
        "ability_links": [
            {
                "ability_dimension_id": 201,
                "evidence_strength": 0.6,
                "response_mode": "structured_response",
            },
            {
                "ability_dimension_id": 202,
                "evidence_strength": 0.9,
                "response_mode": "source_analysis",
            },
        ],
    },
]


def test_edit_and_save(base_url: str, browser) -> None:
    page = browser.new_page(viewport={"width": 1500, "height": 1300})
    open_subjective_links(page, base_url)
    panel = expand_link_panel(page)
    assert_initial_editor_call(page)
    edit_links(panel)

    page.once("dialog", lambda dialog: dialog.dismiss())
    panel.get_by_role("button", name="确认链接，另存未来版本").click()
    save_calls = page.evaluate(
        "window.__linkCalls.filter((item) => item.cmd === 'exam_subjective_link_save')"
    )
    assert save_calls == []
    expect(panel.locator("article.subjective-link-source").nth(1).locator("input").last).to_have_value("0.9")

    page.once("dialog", lambda dialog: dialog.accept())
    panel.get_by_role("button", name="确认链接，另存未来版本").click()
    expect(page.get_by_text(
        "已另存未来作业 v2：2 条知识链接、3 条能力链接；历史成绩保持不变",
        exact=True,
    )).to_be_visible()
    save_calls = page.evaluate(
        "window.__linkCalls.filter((item) => item.cmd === 'exam_subjective_link_save')"
    )
    assert save_calls == [{
        "cmd": "exam_subjective_link_save",
        "args": {"assessmentItemId": 2, "sources": EXPECTED_SOURCES},
    }]
    editor_calls = page.evaluate(
        "window.__linkCalls.filter((item) => item.cmd === 'exam_subjective_link_editor').length"
    )
    assert editor_calls >= 2
    page.screenshot(path="/tmp/jiaofu-r2-subjective-links-saved.png", full_page=True)
    page.close()


def test_save_failure(base_url: str, browser) -> None:
    page = browser.new_page(viewport={"width": 1500, "height": 1300})
    open_subjective_links(page, f"{base_url}?linkSaveFail=1")
    panel = expand_link_panel(page)
    edit_links(panel)
    page.once("dialog", lambda dialog: dialog.accept())
    panel.get_by_role("button", name="确认链接，另存未来版本").click()
    expect(page.get_by_text("LINK_SAVE_FAILED", exact=False)).to_be_visible()
    expect(page.get_by_text("已另存未来作业", exact=False)).to_have_count(0)
    expect(panel.locator("article.subjective-link-source").nth(1).locator("input").last).to_have_value("0.9")
    save_calls = page.evaluate(
        "window.__linkCalls.filter((item) => item.cmd === 'exam_subjective_link_save')"
    )
    assert save_calls == [{
        "cmd": "exam_subjective_link_save",
        "args": {"assessmentItemId": 2, "sources": EXPECTED_SOURCES},
    }]
    page.screenshot(path="/tmp/jiaofu-r2-subjective-links-save-failed.png", full_page=True)
    page.close()


def test_load_failure(base_url: str, browser) -> None:
    page = browser.new_page(viewport={"width": 1500, "height": 1000})
    open_subjective_links(page, f"{base_url}?linkLoadFail=1")
    expect(page.get_by_text("LINK_LOAD_FAILED", exact=False)).to_be_visible()
    expect(page.locator("details.subjective-link-panel")).to_have_count(0)
    assert_initial_editor_call(page)
    page.screenshot(path="/tmp/jiaofu-r2-subjective-links-load-failed.png", full_page=True)
    page.close()


if __name__ == "__main__":
    with sync_playwright() as playwright:
        test_browser = playwright.chromium.launch(headless=True)
        test_edit_and_save("http://127.0.0.1:4173/", test_browser)
        test_save_failure("http://127.0.0.1:4173/", test_browser)
        test_load_failure("http://127.0.0.1:4173/", test_browser)
        test_browser.close()
    print("exam subjective link panel UI characterization: PASS")
