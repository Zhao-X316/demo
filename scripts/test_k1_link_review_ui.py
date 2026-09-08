from ui_navigation import enter_exam as navigate_exam, enter_materials, enter_learning, enter_dashboard
"""K1 L2→L3 知识/能力链接复核浏览器冒烟测试。

验证 AI 只填草稿、缺失槽位阻止确认、老师补齐后才晋级 L3，且请求明确声明
不创建作业、成绩或学习证据。后端事务、跨地图和来源覆盖门禁由 Rust 测试覆盖。
"""

from pathlib import Path

from playwright.sync_api import expect, sync_playwright


OUTPUT_DIR = Path(
    "/Users/zzx/agent-shared/笔记/教辅系统/验收/2026-07-19-k1-link-review"
)

MOCK_SCRIPT = r"""
window.__linkCalls = [];
window.__linkPending = true;
const sources = [
  {
    sourceType: "answer_slot", sourcePublicId: "slot-1",
    label: "第 1 空", detail: "{\"canonical_answers\":[\"1840\"]}",
    orderIndex: 0, requiredForL3: true,
    requiredKnowledgeRelation: "direct_assessment"
  },
  {
    sourceType: "answer_slot", sourcePublicId: "slot-2",
    label: "第 2 空", detail: "{\"canonical_answers\":[\"英国\"]}",
    orderIndex: 1, requiredForL3: true,
    requiredKnowledgeRelation: "direct_assessment"
  }
];
const editor = {
  schemaVersion: 1,
  inputVersion: "k1-link-suggestion-input-v1",
  questionVersionPublicId: "question-fill-1",
  questionContentHash: "a".repeat(64),
  questionType: "fill_blank",
  stem: "鸦片战争爆发于____年，由____发动。",
  materialText: null,
  knowledgeMapPublicId: "map-1",
  knowledgeMapRevision: 1,
  textbookTitle: "中国历史八年级上册",
  sources,
  knowledgeCandidates: [
    {
      publicId: "knowledge-time", code: "K-TIME",
      title: "鸦片战争爆发时间", curriculumTitle: "鸦片战争"
    },
    {
      publicId: "knowledge-country", code: "K-COUNTRY",
      title: "鸦片战争发动国家", curriculumTitle: "鸦片战争"
    }
  ],
  abilityCandidates: [
    {
      publicId: "ability-recall", code: "fact_recall",
      title: "事实识记与提取", description: null
    }
  ]
};
const draft = {
  publicId: "suggestion-1",
  aiRunPublicId: "ai-run-1",
  questionVersionPublicId: "question-fill-1",
  knowledgeMapPublicId: "map-1",
  contentHash: "b".repeat(64),
  createdAt: "2026-07-19T20:00:00Z",
  suggestion: {
    schemaVersion: 1,
    state: "needs_review",
    confidence: 0.82,
    issueCodes: ["slot_2_uncertain"],
    sourceSuggestions: [{
      sourceType: "answer_slot",
      sourcePublicId: "slot-1",
      knowledgeLinks: [{
        knowledgeNodePublicId: "knowledge-time",
        relationType: "direct_assessment",
        confidence: 0.98,
        reason: "第一空直接考查爆发时间"
      }],
      abilityLinks: [{
        abilityDimensionPublicId: "ability-recall",
        evidenceStrength: 0.6,
        responseMode: "recall",
        confidence: 0.96,
        reason: "填空提供回忆型事实证据"
      }]
    }]
  }
};
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__linkCalls.push({ cmd, args });
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "list_profile_scope_options") return [];
    if (cmd === "class_operations_dashboard") {
      return {
        meta: {
          schema_version: 2, rule_version: "m6.1-operations-v2",
          calculated_at: "2026-07-19T20:00:00Z", as_of_date: args.asOfDate,
          recitation_watermark: null, exam_watermark: null
        },
        class: {
          id: 1, name: "八年级一班", term: "2026秋",
          textbook: "中国历史八上", enabled_student_count: 0
        },
        recitation: {
          expected_student_count: 0, completed_student_count: 0,
          expected_task_count: 0, confirmed_task_count: 0, submitted_task_count: 0,
          not_submitted_student_count: 0, pending_teacher_review_count: 0,
          overdue_pending_review_count: 0, recognition_failure_count: 0,
          recognition_processing_count: 0, denominator_note: "暂无任务。"
        },
        exam: {
          active_assessment_count: 0, expected_submission_count: 0,
          submitted_submission_count: 0, missing_submission_count: 0,
          ingesting_attempt_count: 0, grading_attempt_count: 0,
          ready_to_publish_attempt_count: 0, published_submission_count: 0,
          open_pipeline_issue_count: 0, denominator_note: "暂无作业。"
        },
        students: [], actions: []
      };
    }
    if (cmd === "latest_class_profile") return null;
    if (cmd === "list_class_teaching_events"
        || cmd === "list_class_teaching_inputs"
        || cmd === "list_class_action_drafts") return [];
    if (cmd === "k1_question_search") return {items:[],total:0,boundary_note:"测试资料"};
    if (cmd === "k1_blueprint_options") {
      return {
        classes: [{ id: 1, name: "八年级一班", term: "2026秋" }],
        knowledge_maps: [{ public_id: "map-1", title: "中国历史八年级上册", revision: 1 }],
        curriculum_nodes: [],
        knowledge_nodes: [],
        question_types: ["single", "multiple", "true_false", "fill_blank", "short_answer"]
      };
    }
    if (cmd === "k1_blueprint_list"
        || cmd === "k1_source_inbox"
        || cmd === "k1_answer_targets"
        || cmd === "k1_answer_inbox") return [];
    if (cmd === "k1_link_review_catalog") {
      return {
        maps: [{
          publicId: "map-1", title: "中国历史八年级上册", revision: 1,
          subjectTitle: "历史", knowledgeCount: 2, abilityCount: 1
        }]
      };
    }
    if (cmd === "k1_link_review_inbox") {
      return window.__linkPending ? [{
        questionVersionPublicId: "question-fill-1",
        questionContentHash: "a".repeat(64),
        questionType: "fill_blank",
        stem: "鸦片战争爆发于____年，由____发动。",
        materialText: null,
        maxScore: 2,
        qualityLevel: "L2",
        sources,
        latestSuggestion: null,
        createdAt: "2026-07-19T19:00:00Z"
      }] : [];
    }
    if (cmd === "k1_link_review_editor") return editor;
    if (cmd === "k1_link_suggest") {
      return {
        aiRunId: 1, status: "succeeded",
        suggestion: draft, output: draft.suggestion, failure: null
      };
    }
    if (cmd === "k1_link_confirm") {
      window.__linkPending = false;
      return {
        publicId: "review-1",
        questionVersionPublicId: "question-fill-1",
        knowledgeMapPublicId: "map-1",
        suggestionDraftPublicId: "suggestion-1",
        resultLinkSetPublicId: "link-set-1",
        resultQuality: "L3",
        knowledgeLinkCount: 2,
        abilityLinkCount: 2,
        reviewedBy: "local_teacher",
        note: null,
        reviewedAt: "2026-07-19T20:05:00Z",
        createsAssessment: false,
        createsGrade: false,
        createsLearningEvidence: false
      };
    }
    return [];
  }
};
"""


def test_k1_link_review(base_url: str) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1500, "height": 1100})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        enter_materials(page)
        page.get_by_role("button", name="导入题目").click()

        expect(page.get_by_text("关联知识点与能力", exact=True)).to_be_visible()
        expect(page.get_by_text("1 道 L2 待关联", exact=True)).to_be_visible()
        confirm = page.get_by_role("button", name="确认链接并晋级 L3")
        expect(confirm).to_be_disabled()
        page.screenshot(path=str(OUTPUT_DIR / "01-link-empty.png"), full_page=True)

        page.get_by_role("button", name="AI 整理链接草稿").click()
        expect(page.get_by_text("AI 草稿 82%", exact=True)).to_be_visible()
        expect(
            page.locator(".link-chip")
            .filter(has_text="鸦片战争爆发时间")
            .first
        ).to_be_visible()
        expect(confirm).to_be_disabled()
        page.screenshot(path=str(OUTPUT_DIR / "02-link-ai-draft.png"), full_page=True)

        second = page.locator(".knowledge-link-source").filter(has_text="第 2 空")
        second.get_by_label("第 2 空 知识点").select_option("knowledge-country")
        second.get_by_role("button", name="添加知识点").click()
        second.get_by_role("button", name="添加能力").click()
        expect(page.get_by_text("必需来源已覆盖", exact=False)).to_be_visible()
        expect(confirm).to_be_enabled()
        page.screenshot(path=str(OUTPUT_DIR / "03-link-teacher-complete.png"), full_page=True)

        confirm.click()
        expect(page.get_by_text("题目已晋级 L3", exact=False)).to_be_visible()
        expect(page.get_by_text("暂无待关联题目", exact=False)).to_be_visible()

        calls = page.evaluate(
            """window.__linkCalls.filter((item) =>
              item.cmd === "k1_link_suggest" || item.cmd === "k1_link_confirm")"""
        )
        assert [item["cmd"] for item in calls] == ["k1_link_suggest", "k1_link_confirm"]
        payload = calls[1]["args"]["input"]
        assert payload["suggestionDraftPublicId"] == "suggestion-1"
        assert payload["expectedSuggestionContentHash"] == "b" * 64
        assert len(payload["sources"]) == 2
        assert all(len(source["knowledgeLinks"]) == 1 for source in payload["sources"])
        assert all(len(source["abilityLinks"]) == 1 for source in payload["sources"])
        assert payload["sources"][1]["knowledgeLinks"][0] == {
            "knowledgeNodePublicId": "knowledge-country",
            "relationType": "direct_assessment",
        }
        page.screenshot(path=str(OUTPUT_DIR / "04-link-complete.png"), full_page=True)
        browser.close()


if __name__ == "__main__":
    test_k1_link_review("http://127.0.0.1:4173")
    print("K1 link review UI smoke: PASS")
