"""K1 可解释题库组卷、换题排版与打印浏览器冒烟测试。

通过浏览器端 Tauri invoke mock 验证：一页式蓝图输入、L3 候选解释、
题型/总分/知识点对齐门禁、老师确认、冻结作业摘要、同槽位换题、
简单排序/分页，以及题卷和答案卷分别保存。
后端固定版本、事务、幂等和旧预览失效由 Rust 单元测试覆盖。
"""

from playwright.sync_api import expect, sync_playwright


MOCK_SCRIPT = r"""
window.__blueprintCalls = [];
window.__assemblies = [];
window.__duplicateDecision = null;
window.__candidatePending = true;
window.__paperRevision = 0;
window.__paperItems = null;
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  unregisterCallback: () => undefined,
  convertFileSrc: (path) => path,
  invoke: async (cmd, args = {}) => {
    window.__blueprintCalls.push({ cmd, args });
    if (cmd === "classes_list") {
      return [{ id: 1, name: "八年级一班", term: "2026秋", textbook: "中国历史八上" }];
    }
    if (cmd === "list_profile_scope_options") return [];
    if (cmd === "class_operations_dashboard") {
      return {
        meta: {
          schema_version: 2, rule_version: "m6.1-operations-v2",
          calculated_at: "2026-07-19T12:00:00Z", as_of_date: args.asOfDate,
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
    if (cmd === "k1_candidate_review_list") {
      return {
        schema_version: 1,
        rule_version: "m2.5-candidate-review-v1",
        calculated_at: "2026-07-19T12:00:00Z",
        pending_count: window.__candidatePending ? 1 : 0,
        boundary_note: "这里只整理已通过隐私门禁的老师私有 C0 候选；确认到 L1 不会切换当前作业、重算成绩或进入学习图谱。",
        items: window.__candidatePending ? [{
          candidate_public_id: "candidate-1",
          source_type: "student_paper",
          privacy_status: "text_only",
          question_type: "single",
          stem: "洋务运动后期提出的口号是？",
          material_text: null,
          max_score: 2,
          options: [
            { label: "A", content: "自强", order_index: 0 },
            { label: "B", content: "求富", order_index: 1 }
          ],
          content_hash: "c".repeat(64),
          quality_issues: ["standard_answer_unconfirmed"],
          source_question_version_public_id: "candidate-version-1",
          source_quality_level: "C0",
          created_at: "2026-07-19T11:50:00Z",
          review_action: null,
          result_question_version_public_id: null,
          reviewed_at: null
        }] : []
      };
    }
    if (cmd === "k1_candidate_promote_l1") {
      window.__candidatePending = false;
      return {
        public_id: "candidate-review-1",
        candidate_public_id: "candidate-1",
        action: "promote_l1",
        source_question_version_public_id: "candidate-version-1",
        result_question_version_public_id: "candidate-version-2",
        result_answer_key_version_public_id: "candidate-answer-1",
        result_quality_level: "L1",
        reviewed_by: "local_teacher",
        reviewed_at: "2026-07-19T12:02:00Z",
        current_assessment_rebound: false,
        boundary_note: "只更新个人题库候选；当前作业固定题目、成绩和学习证据均未切换。"
      };
    }
    if (cmd === "k1_question_search") {
      const candidate = {
        question_version_public_id: "question-2",
        stem: "鸦片战争开始的时间是？",
        match_kind: "similar",
        similarity: 0.91,
        decision: window.__duplicateDecision,
        decision_note: null,
        decision_revision: window.__duplicateDecision ? 1 : null
      };
      return {
        schema_version: 1,
        rule_version: "k1-structured-search-v1",
        calculated_at: "2026-07-19T12:00:00Z",
        total: 2,
        limit: 50,
        offset: 0,
        boundary_note: "只显示我的当前题目和已发布官方题；相似结果只供老师归类，不会自动合并或改写历史作业。",
        items: [
          {
            question_public_id: "identity-1",
            question_version_public_id: "question-1",
            revision: 1,
            owner_scope: "personal",
            owner_label: "我的题库",
            question_type: "single",
            stem: "鸦片战争爆发于哪一年？",
            material_text: null,
            max_score: 1,
            quality_level: "L3",
            state: "published",
            options: [{ label: "A", content: "1840年" }, { label: "B", content: "1842年" }],
            knowledge_nodes: [{ public_id: "knowledge-1", title: "鸦片战争爆发时间", relation_type: "direct_assessment" }],
            ability_dimensions: [{ public_id: "ability-1", title: "事实识记与提取", evidence_strength: 0.5, response_mode: "recognition" }],
            assessment_usage_count: 2,
            duplicate_candidates: [candidate]
          },
          {
            question_public_id: "identity-2",
            question_version_public_id: "question-2",
            revision: 1,
            owner_scope: "official",
            owner_label: "官方精选",
            question_type: "single",
            stem: "鸦片战争开始的时间是？",
            material_text: null,
            max_score: 1,
            quality_level: "L3",
            state: "published",
            options: [{ label: "A", content: "1840年" }, { label: "B", content: "1842年" }],
            knowledge_nodes: [{ public_id: "knowledge-1", title: "鸦片战争爆发时间", relation_type: "direct_assessment" }],
            ability_dimensions: [],
            assessment_usage_count: 1,
            duplicate_candidates: []
          }
        ]
      };
    }
    if (cmd === "k1_question_semantic_search") {
      return {
        aiRunId: 91,
        status: "succeeded",
        state: "ready",
        confidence: 0.95,
        issueCodes: [],
        items: [{
          candidate: {
            questionVersionPublicId: "question-1",
            revision: 1,
            ownerScope: "personal",
            ownerLabel: "我的题库",
            questionType: "single",
            stem: "鸦片战争爆发于哪一年？",
            materialText: null,
            maxScore: 1,
            qualityLevel: "L3",
            options: [
              { label: "A", content: "1840年" },
              { label: "B", content: "1842年" }
            ],
            knowledgeTitles: ["鸦片战争爆发时间"],
            abilityTitles: ["事实识记与提取"]
          },
          score: 0.97,
          reason: "直接考查中国近代史开端的标志性事件"
        }],
        failure: null,
        catalogSnapshotHash: "a".repeat(64),
        boundaryNote: "本机先按老师权限和结构化条件冻结 2 道候选；AI 只在该清单内按意思排序。结果仅供选题。"
      };
    }
    if (cmd === "k1_duplicate_review") {
      window.__duplicateDecision = args.input.decision;
      return {
        public_id: "decision-1",
        left_question_version_public_id: "question-1",
        right_question_version_public_id: "question-2",
        revision: 1,
        match_kind: "similar",
        similarity: 0.91,
        decision: args.input.decision,
        note: null,
        decided_by: "local_teacher",
        decided_at: "2026-07-19T12:01:00Z",
        state: "active",
        identity_changed: false
      };
    }
    if (cmd === "k1_blueprint_options") {
      return {
        classes: [{ id: 1, name: "八年级一班", term: "2026秋" }],
        knowledge_maps: [{ public_id: "map-1", title: "中国历史八年级上册", revision: 1 }],
        curriculum_nodes: [{
          public_id: "lesson-1", knowledge_map_public_id: "map-1",
          parent_public_id: null, node_type: "lesson", title: "鸦片战争", order_index: 1
        }],
        knowledge_nodes: [{
          public_id: "knowledge-1", knowledge_map_public_id: "map-1",
          curriculum_node_public_id: "lesson-1", title: "鸦片战争爆发时间", order_index: 1
        }],
        question_types: ["single", "multiple", "true_false", "fill_blank", "short_answer"]
      };
    }
    if (cmd === "k1_link_review_catalog") return { maps: [] };
    if (cmd === "k1_link_review_inbox") return [];
    if (cmd === "k1_blueprint_list") return window.__assemblies;
    if (cmd === "k1_blueprint_preview") {
      return {
        schema_version: 1,
        rule_version: "k1-explainable-blueprint-v1",
        calculated_at: "2026-07-19T12:00:00Z",
        class_id: 1,
        class_name: "八年级一班",
        knowledge_map_public_id: "map-1",
        knowledge_map_title: "中国历史八年级上册",
        curriculum_node_public_id: "lesson-1",
        curriculum_node_title: "鸦片战争",
        total_score: 2,
        question_type_targets: [
          { question_type: "single", count: 2 },
          { question_type: "multiple", count: 0 },
          { question_type: "true_false", count: 0 },
          { question_type: "fill_blank", count: 0 },
          { question_type: "short_answer", count: 0 }
        ],
        required_knowledge_node_public_ids: ["knowledge-1"],
        preview_hash: "a".repeat(64),
        candidates: [
          {
            question_version_public_id: "question-1",
            question_type: "single",
            stem: "鸦片战争爆发于哪一年？",
            material_text: null,
            score: 1,
            quality_level: "L3",
            knowledge_nodes: [{
              public_id: "knowledge-1", title: "鸦片战争爆发时间",
              relation_type: "direct_assessment"
            }],
            ability_dimensions: [{
              public_id: "ability-1", title: "事实识记与提取",
              evidence_strength: 0.5, response_mode: "recognition"
            }],
            explanation: "已发布 L3 题；直接考查：鸦片战争爆发时间；能力证据：事实识记与提取"
          },
          {
            question_version_public_id: "question-2",
            question_type: "single",
            stem: "鸦片战争开始的时间是？",
            material_text: null,
            score: 1,
            quality_level: "L3",
            knowledge_nodes: [{
              public_id: "knowledge-1", title: "鸦片战争爆发时间",
              relation_type: "direct_assessment"
            }],
            ability_dimensions: [{
              public_id: "ability-1", title: "事实识记与提取",
              evidence_strength: 0.5, response_mode: "recognition"
            }],
            explanation: "已发布 L3 题；直接考查：鸦片战争爆发时间；能力证据：事实识记与提取"
          }
        ],
        recommended_question_version_public_ids: ["question-1", "question-2"],
        recommended_summary: {
          selected_count: 2,
          selected_score: 2,
          selected_type_counts: [
            { question_type: "single", count: 2 },
            { question_type: "multiple", count: 0 },
            { question_type: "true_false", count: 0 },
            { question_type: "fill_blank", count: 0 },
            { question_type: "short_answer", count: 0 }
          ],
          covered_required_knowledge_node_public_ids: ["knowledge-1"]
        },
        can_confirm: true,
        blockers: [],
        warnings: [],
        boundary_note: "只使用已发布 L3+ 题目；确认后仅冻结全班作业版本，不创建学生作答、不发布成绩。"
      };
    }
    if (cmd === "k1_blueprint_confirm") {
      const assembly = {
        public_id: "assembly-1",
        class_id: 1,
        class_name: "八年级一班",
        knowledge_map_public_id: "map-1",
        knowledge_map_title: "中国历史八年级上册",
        curriculum_node_public_id: "lesson-1",
        curriculum_node_title: "鸦片战争",
        title: args.input.title,
        total_score: 2,
        question_type_targets: [{ question_type: "single", count: 2 }],
        required_knowledge_node_public_ids: ["knowledge-1"],
        preview_hash: "a".repeat(64),
        selected_set_hash: "b".repeat(64),
        assessment_public_id: "assessment-1",
        assessment_version_public_id: "assessment-version-1",
        state: "confirmed",
        confirmed_by: "local_teacher",
        confirmed_at: "2026-07-19T12:05:00Z",
        items: [
          {
            order_index: 0, question_version_public_id: "question-1",
            question_type: "single", stem: "鸦片战争爆发于哪一年？",
            score: 1, explanation: "直接考查鸦片战争爆发时间"
          },
          {
            order_index: 1, question_version_public_id: "question-2",
            question_type: "single", stem: "鸦片战争开始的时间是？",
            score: 1, explanation: "直接考查鸦片战争爆发时间"
          }
        ]
      };
      window.__assemblies = [assembly];
      return assembly;
    }
    if (cmd === "k1_blueprint_paper_editor") {
      const initialItems = [
        {
          source_slot_order_index: 0, order_index: 0,
          question_version_public_id: "question-1", question_type: "single",
          stem: "鸦片战争爆发于哪一年？", material_text: null,
          score: 1, page_break_before: false
        },
        {
          source_slot_order_index: 1, order_index: 1,
          question_version_public_id: "question-2", question_type: "single",
          stem: "鸦片战争开始的时间是？", material_text: null,
          score: 1, page_break_before: false
        }
      ];
      const candidates = [
        {
          question_version_public_id: "question-1", question_type: "single",
          stem: "鸦片战争爆发于哪一年？", material_text: null, score: 1,
          quality_level: "L3", knowledge_nodes: [], ability_dimensions: [],
          explanation: "合格题"
        },
        {
          question_version_public_id: "question-2", question_type: "single",
          stem: "鸦片战争开始的时间是？", material_text: null, score: 1,
          quality_level: "L3", knowledge_nodes: [], ability_dimensions: [],
          explanation: "合格题"
        },
        {
          question_version_public_id: "question-3", question_type: "single",
          stem: "中国近代史开端对应哪次战争？", material_text: null, score: 1,
          quality_level: "L3", knowledge_nodes: [], ability_dimensions: [],
          explanation: "合格替换题"
        }
      ];
      return {
        schema_version: 1,
        rule_version: "k1-blueprint-paper-v1",
        assembly_public_id: "assembly-1",
        title: "课堂练习",
        class_name: "八年级一班",
        knowledge_map_title: "中国历史八年级上册",
        curriculum_node_title: "鸦片战争",
        current_edition_public_id: window.__paperRevision ? "paper-edition-1" : null,
        current_revision: window.__paperRevision,
        source_assessment_version_public_id:
          window.__paperRevision ? "assessment-version-2" : "assessment-version-1",
        required_knowledge_node_public_ids: ["knowledge-1"],
        items: window.__paperItems || initialItems,
        candidates,
        boundary_note:
          "换题只允许同题型、同分值的当前 L3/L4 题；确认会追加新作业版本和打印快照。"
      };
    }
    if (cmd === "k1_blueprint_paper_confirm") {
      window.__paperRevision = 1;
      window.__paperItems = args.input.items.map((item, index) => {
        const source = item.questionVersionPublicId === "question-3"
          ? { stem: "中国近代史开端对应哪次战争？" }
          : item.questionVersionPublicId === "question-2"
            ? { stem: "鸦片战争开始的时间是？" }
            : { stem: "鸦片战争爆发于哪一年？" };
        return {
          source_slot_order_index: item.sourceSlotOrderIndex,
          order_index: index,
          question_version_public_id: item.questionVersionPublicId,
          question_type: "single",
          stem: source.stem,
          material_text: null,
          score: 1,
          page_break_before: item.pageBreakBefore
        };
      });
      return {
        public_id: "paper-edition-1",
        assembly_public_id: "assembly-1",
        revision: 1,
        title: args.input.title,
        assessment_public_id: "assessment-1",
        source_assessment_version_public_id: "assessment-version-1",
        assessment_version_public_id: "assessment-version-2",
        supersedes_edition_public_id: null,
        item_set_hash: "d".repeat(64),
        question_html_sha256: "e".repeat(64),
        answer_html_sha256: "f".repeat(64),
        suggested_question_file_name: "课堂练习-题卷-第1版.html",
        suggested_answer_file_name: "课堂练习-答案-第1版.html",
        state: "confirmed",
        confirmed_by: "local_teacher",
        confirmed_at: "2026-07-19T12:10:00Z",
        items: window.__paperItems
      };
    }
    if (cmd === "plugin:dialog|save") {
      return `/tmp/${args.options.defaultPath}`;
    }
    if (cmd === "k1_blueprint_paper_write") {
      return {
        edition_public_id: args.editionPublicId,
        export_kind: args.exportKind,
        file_name: args.outputPath.split("/").pop(),
        byte_size: 2048,
        sha256: args.exportKind === "question" ? "e".repeat(64) : "f".repeat(64)
      };
    }
    return [];
  }
};
"""


def test_question_bank(base_url: str) -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.add_init_script(MOCK_SCRIPT)
        page.goto(base_url)
        page.wait_for_load_state("networkidle")

        page.locator(".mod-row").filter(has_text="改作业").click()
        page.get_by_role("button", name="题目与题库").click()
        expect(page.get_by_role("heading", name="题目与知识库")).to_be_visible()
        page.get_by_role("button", name="找题与查重").click()
        expect(page.get_by_role("button", name="找题与查重")).to_have_class("tab active")
        expect(page.get_by_text("找到 2 道题", exact=True)).to_be_visible()
        expect(page.locator(".question-search-card")).to_have_count(2)
        expect(page.get_by_text("疑似同题变式 · 91%", exact=True)).to_be_visible()
        page.get_by_role("button", name="标记为同题变式（仅归类，不合并）").click()
        expect(page.get_by_text("已标记为同题变式；两道题仍保持独立", exact=False)).to_be_visible()
        expect(page.get_by_text("老师已归为同题变式 · 91%", exact=True)).to_be_visible()
        duplicate_calls = page.evaluate(
            """window.__blueprintCalls.filter((item) => item.cmd === "k1_duplicate_review")"""
        )
        assert duplicate_calls[0]["args"]["input"]["decision"] == "same_family"
        assert duplicate_calls[0]["args"]["input"]["leftQuestionVersionPublicId"] == "question-1"
        assert duplicate_calls[0]["args"]["input"]["rightQuestionVersionPublicId"] == "question-2"
        page.screenshot(path="/tmp/jiaofu-question-search.png", full_page=True)

        page.get_by_label("查找方式").select_option("semantic")
        page.get_by_label("想找什么题").fill("找考查中国近代史开端的题")
        page.get_by_role("button", name="按意思查找").click()
        expect(page.get_by_text("按意思找到 1 道题", exact=True)).to_be_visible()
        expect(page.get_by_text("语义相关 97%", exact=True)).to_be_visible()
        expect(page.get_by_text("直接考查中国近代史开端的标志性事件", exact=True)).to_be_visible()
        semantic_calls = page.evaluate(
            """window.__blueprintCalls.filter((item) =>
              item.cmd === "k1_question_semantic_search")"""
        )
        assert len(semantic_calls) == 1
        semantic_input = semantic_calls[0]["args"]["input"]
        assert semantic_input["search"]["query"] == "找考查中国近代史开端的题"
        assert semantic_input["search"]["limit"] == 20
        assert semantic_input["search"]["duplicateOnly"] is False
        assert semantic_input["requestKey"].startswith("k1-semantic-search-")
        page.screenshot(path="/tmp/jiaofu-question-semantic-search.png", full_page=True)

        page.get_by_role("button", name="待整理新题").click()
        expect(page.get_by_text("1 道待整理", exact=True)).to_be_visible()
        expect(page.locator(".candidate-list-item").filter(
            has_text="洋务运动后期提出的口号是？"
        )).to_be_visible()
        page.get_by_placeholder("填写一个正确选项字母，如 B").fill("B")
        page.screenshot(path="/tmp/jiaofu-question-candidate-review-editor.png", full_page=True)
        page.get_by_role("button", name="确认并收入我的题库").click()
        expect(page.get_by_text("已收入个人题库 L1；当前作业和历史成绩未切换。", exact=True)).to_be_visible()
        expect(page.get_by_text("当前没有待整理题目。", exact=False)).to_be_visible()
        candidate_calls = page.evaluate(
            """window.__blueprintCalls.filter((item) => item.cmd === "k1_candidate_promote_l1")"""
        )
        assert len(candidate_calls) == 1
        candidate_input = candidate_calls[0]["args"]["input"]
        assert candidate_input["candidatePublicId"] == "candidate-1"
        assert candidate_input["answerText"] == "B"
        assert candidate_input["options"] == [
            {"label": "A", "content": "自强", "orderIndex": 0},
            {"label": "B", "content": "求富", "orderIndex": 1},
        ]
        page.screenshot(path="/tmp/jiaofu-question-candidate-review.png", full_page=True)

        page.get_by_role("button", name="按蓝图组卷").click()
        expect(page.get_by_text("1. 这次要练什么", exact=True)).to_be_visible()

        page.locator(".blueprint-fields select").nth(2).select_option("lesson-1")
        page.locator(".blueprint-type-row input").nth(0).fill("2")
        page.locator(".blueprint-type-row input").nth(5).fill("2")
        page.get_by_role("button", name="鸦片战争爆发时间").click()
        page.get_by_role("button", name="整理可用题目").click()

        expect(page.get_by_text("找到 2 道合格题", exact=False)).to_be_visible()
        expect(page.locator(".blueprint-candidate")).to_have_count(2)
        expect(page.locator(".blueprint-candidate input:checked")).to_have_count(2)
        expect(page.get_by_text("当前选中 2 道 · 2.0 / 2 分", exact=True)).to_be_visible()
        expect(page.get_by_text("知识 · 鸦片战争爆发时间", exact=True).first).to_be_visible()
        expect(page.get_by_text("能力 · 事实识记与提取", exact=True).first).to_be_visible()
        confirm = page.get_by_role("button", name="确认并建立作业")
        expect(confirm).to_be_enabled()

        page.locator(".blueprint-candidate input").nth(1).uncheck()
        expect(confirm).to_be_disabled()
        expect(page.get_by_text("请让题型数量、总分和必覆盖知识点全部对齐", exact=True)).to_be_visible()
        page.locator(".blueprint-candidate input").nth(1).check()
        expect(confirm).to_be_enabled()
        confirm.click()

        expect(page.get_by_text("已冻结“课堂练习”：2 道，2 分。", exact=True)).to_be_visible()
        expect(page.get_by_text("最近确认的组卷", exact=True)).to_be_visible()
        expect(page.locator(".blueprint-history-row")).to_have_count(1)
        expect(page.get_by_text("不代表学生已经提交或成绩已经发布", exact=False)).to_be_visible()

        page.get_by_role("button", name="换题与打印").click()
        expect(page.get_by_text("调整题目并生成打印稿", exact=True)).to_be_visible()
        expect(page.locator(".blueprint-paper-item")).to_have_count(2)
        page.get_by_label("第 1 题换题").select_option("question-3")
        expect(page.get_by_role("paragraph").filter(
            has_text="中国近代史开端对应哪次战争？"
        )).to_be_visible()
        page.get_by_label("第 1 题下移").click()
        page.locator(".blueprint-paper-item").nth(1).get_by_text("从新页开始").click()
        page.get_by_role("button", name="确认新版并生成打印稿").click()
        expect(page.get_by_text("第 1 版已冻结。现在可分别保存题卷和答案卷。", exact=True)).to_be_visible()
        expect(page.get_by_role("button", name="保存题卷")).to_be_visible()
        expect(page.get_by_role("button", name="保存答案卷")).to_be_visible()

        paper_calls = page.evaluate(
            """window.__blueprintCalls.filter((item) =>
              item.cmd === "k1_blueprint_paper_confirm")"""
        )
        assert len(paper_calls) == 1
        paper_input = paper_calls[0]["args"]["input"]
        assert paper_input["expectedSourceAssessmentVersionPublicId"] == "assessment-version-1"
        assert paper_input["items"] == [
            {
                "sourceSlotOrderIndex": 1,
                "questionVersionPublicId": "question-2",
                "pageBreakBefore": False,
            },
            {
                "sourceSlotOrderIndex": 0,
                "questionVersionPublicId": "question-3",
                "pageBreakBefore": True,
            },
        ]
        page.get_by_role("button", name="保存题卷").click()
        expect(page.get_by_text("已保存 课堂练习-题卷-第1版.html", exact=False)).to_be_visible()
        page.get_by_role("button", name="保存答案卷").click()
        expect(page.get_by_text("已保存 课堂练习-答案-第1版.html", exact=False)).to_be_visible()
        write_calls = page.evaluate(
            """window.__blueprintCalls.filter((item) =>
              item.cmd === "k1_blueprint_paper_write")"""
        )
        assert [item["args"]["exportKind"] for item in write_calls] == [
            "question",
            "answer",
        ]
        page.screenshot(path="/tmp/jiaofu-blueprint-paper.png", full_page=True)

        calls = page.evaluate(
            """window.__blueprintCalls.filter((item) =>
              item.cmd === "k1_blueprint_preview" || item.cmd === "k1_blueprint_confirm")"""
        )
        assert [item["cmd"] for item in calls] == [
            "k1_blueprint_preview",
            "k1_blueprint_confirm",
        ]
        preview_input = calls[0]["args"]["input"]
        assert preview_input["classId"] == 1
        assert preview_input["curriculumNodePublicId"] == "lesson-1"
        assert preview_input["totalScore"] == 2
        assert preview_input["requiredKnowledgeNodePublicIds"] == ["knowledge-1"]
        confirm_input = calls[1]["args"]["input"]
        assert confirm_input["expectedPreviewHash"] == "a" * 64
        assert confirm_input["selectedQuestionVersionPublicIds"] == ["question-1", "question-2"]

        page.screenshot(path="/tmp/jiaofu-question-bank.png", full_page=True)
        browser.close()


if __name__ == "__main__":
    test_question_bank("http://127.0.0.1:4173")
    print("question bank UI smoke: PASS")
